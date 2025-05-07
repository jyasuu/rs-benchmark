use axum::{
    extract::{Query as AxumQuery, State},
    http::StatusCode,
    Json, // Added for JSON responses
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_postgres::{Client, NoTls, Error as PgError};
use dotenv::dotenv;
use tracing::{error, info, debug};
use elasticsearch::{Elasticsearch, Error as EsError, SearchParts, http::transport::Transport};
use std::fmt;
use std::error::Error as StdError;

// Name of the table in PostgreSQL, consistent with your main.rs
const PG_TABLE_NAME: &str = "documents_jsonb";
const ES_INDEX_NAME: &str = "documents_jsonb"; // Consistent with your main.rs

#[derive(Deserialize, Debug)]
struct ApiParams {
    tag: String,
}

struct AppState {
    db_client: Client,
    es_client: Elasticsearch,
}

#[derive(Debug)] // Ensure Debug is derived
enum ApiError {
    Database(PgError),
    Config(String),
    Elasticsearch(EsError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::Database(e) => {
                error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal server error: {}", e))
            }
            ApiError::Elasticsearch(e) => {
                error!("Elasticsearch error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal server error (ES): {}", e))
            }
            ApiError::Config(e) => {
                error!("Configuration error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Configuration error: {}", e))
            }
        };
        (status, error_message).into_response()
    }
}

// Implement Display for ApiError
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Database(e) => write!(f, "Database error: {}", e),
            ApiError::Elasticsearch(e) => write!(f, "Elasticsearch error: {}", e),
            ApiError::Config(s) => write!(f, "Configuration error: {}", s),
        }
    }
}

// Implement StdError for ApiError
impl StdError for ApiError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            ApiError::Database(e) => Some(e),
            ApiError::Elasticsearch(e) => Some(e),
            ApiError::Config(_) => None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    // Initialize tracing (logging)
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Connect to PostgreSQL
    let database_url = env::var("DATABASE_URL")
        .map_err(|e| ApiError::Config(format!("DATABASE_URL not set: {}", e)))?;
    
    let (pg_client, connection) = tokio_postgres::connect(&database_url, NoTls).await
        .map_err(ApiError::Database)?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("PostgreSQL connection error: {}", e);
        }
    });
    info!("Successfully connected to PostgreSQL.");

    // Connect to Elasticsearch
    let es_url = env::var("ELASTICSEARCH_URL")
        .map_err(|_| ApiError::Config("ELASTICSEARCH_URL environment variable not set".to_string()))?;
    let es_transport = Transport::single_node(&es_url)
        .map_err(ApiError::Elasticsearch)?;
    let es_client = Elasticsearch::new(es_transport);
    info!("Elasticsearch client configured for URL: {}", es_url);

    let shared_state = Arc::new(AppState { db_client: pg_client, es_client });

    let app = Router::new()
        .route("/api/postgres", get(postgres_handler))
        .route("/api/elasticsearch", get(elasticsearch_handler))
        .with_state(shared_state);

    // run it with hyper on localhost:4444
    // Note: Your k6 script targets /api, you might need to update it to /api/postgres or /api/elasticsearch
    let addr = SocketAddr::from(([0, 0, 0, 0], 4444));
    info!("API server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn postgres_handler(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<ApiParams>,
) -> Result<Json<Vec<String>>, ApiError> {
    debug!("Received request for tag: {}", params.tag);

    // Construct the query parameter for JSONB: ["tag_value"]
    let tag_param_json = json!([params.tag]);

    let query_sql = format!(
        "SELECT data ->> 'title' AS title FROM {} WHERE data -> 'tags' @> $1::jsonb LIMIT 100",
        PG_TABLE_NAME
    );

    match state.db_client.query(&query_sql, &[&tag_param_json]).await {
        Ok(rows) => {
            let titles: Vec<String> = rows.iter().filter_map(|row| row.get("title")).collect();
            if titles.is_empty() {
                debug!("No data found for tag: {}, returning empty list.", params.tag);
            } else {
                debug!("Found {} titles for tag: {}", titles.len(), params.tag);
            }
            Ok(Json(titles)) // Axum handles serializing Vec<String> to JSON and sets 200 OK
        }
        Err(e) => { // Query execution failed
            error!("Database query failed for tag {}: {}", params.tag, e);
            Err(ApiError::Database(e))
        }
    }
}

async fn elasticsearch_handler(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<ApiParams>,
) -> Result<Json<Vec<String>>, ApiError> {
    debug!("Received Elasticsearch request for tag: {}", params.tag);

    let search_response = state
        .es_client
        .search(SearchParts::Index(&[ES_INDEX_NAME]))
        .body(json!({
            "_source": ["title"], // Fetch only the title
            "query": {
                "term": { // Assumes 'tags' field is mapped as 'keyword' for exact matching
                    "tags": params.tag
                }
            },
            "size": 100 // Limit results, similar to PostgreSQL query
        }))
        .send()
        .await
        .map_err(ApiError::Elasticsearch)?;

    // elasticsearch-rs client's send() should return Err for non-2xx HTTP status codes.
    // If Ok, we proceed to parse.
    let response_body = search_response
        .json::<serde_json::Value>()
        .await
        .map_err(ApiError::Elasticsearch)?;

    let mut titles: Vec<String> = Vec::new();
    if let Some(hits_array) = response_body.get("hits").and_then(|h| h.get("hits")).and_then(|h_inner| h_inner.as_array()) {
        for hit in hits_array {
            if let Some(source) = hit.get("_source") {
                if let Some(title_val) = source.get("title") {
                    if let Some(title_str) = title_val.as_str() {
                        titles.push(title_str.to_string());
                    }
                }
            }
        }
    }

    if titles.is_empty() {
        debug!("No Elasticsearch data found for tag: {}, returning empty list.", params.tag);
    } else {
        debug!("Found {} titles via Elasticsearch for tag: {}", titles.len(), params.tag);
    }
    Ok(Json(titles))
}