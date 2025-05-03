// src/main.rs
use std::time::{Duration, Instant};
use std::env;
use dotenv::dotenv;
use elasticsearch::{
    Elasticsearch, BulkOperation, Error as EsError, http::transport::Transport, SearchParts,
    BulkParts, indices::{IndicesExistsParts, IndicesCreateParts, IndicesRefreshParts},
};
use serde_json::{Value, json}; // Keep Value, add json macro usage
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_postgres::{Client, NoTls, Error as PgError};
use tokio_postgres::types::{Type, ToSql}; // Add ToSql
use futures_util::pin_mut;
use tokio_postgres::binary_copy::BinaryCopyInWriter;

// Declare the module
mod generate_data;

const BATCH_SIZE: usize = 1000; // Increase batch size for COPY/Bulk
const ES_INDEX_NAME: &str = "documents_jsonb"; // New index name
const PG_TABLE_NAME: &str = "documents_jsonb"; // New table name

#[derive(Error, Debug)]
enum BenchmarkError {
    #[error("Postgres Error: {0}")]
    Postgres(#[from] PgError),
    #[error("Elasticsearch Error: {0}")]
    Elasticsearch(#[from] EsError),
    #[error("JSON Error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Environment variable not set: {0}")]
    EnvVar(String),
    #[error("URL Parse Error: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("Elasticsearch Bulk Operation Error: {0}")]
    EsBulkError(String),
    #[error("Data Conversion Error: {0}")]
    Conversion(String),
}

// Updated struct to match the new JSON structure
// We'll primarily work with serde_json::Value for flexibility,
// but having a struct can be useful for validation or specific cases.
#[derive(Serialize, Deserialize, Debug, Clone)] // Add Clone
struct Document {
    title: String,
    content: String,
    created_at: chrono::DateTime<chrono::Utc>,
    tags: Vec<String>,
    attributes: Value, // Use Value for flexible attributes object
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    println!("Starting benchmark with JSONB focus...");

    // --- Connections (remain the same) ---
    println!("Connecting to databases...");
    let pg_client = connect_postgres().await?;
    let transport = Transport::single_node(
        &env::var("ELASTICSEARCH_URL")
            .unwrap_or_else(|_| "http://localhost:9200".to_string())
    )?;
    let es_client = Elasticsearch::new(transport);
    println!("Connections established.");

    // --- Setup (modified for JSONB and new ES mapping) ---
    println!("Setting up database schemas...");
    setup_postgres(&pg_client).await?;
    setup_elasticsearch(&es_client).await?;
    println!("Schemas ready.");

    // --- Data Generation (uses updated generate_data.rs) ---
    let data_count = 100_000; // Adjust as needed
    println!("Generating {} documents...", data_count);
    let start_gen = Instant::now();
    let docs_json_strings = generate_data::generate_documents(data_count).await;
    println!("Data generation took: {:?}", start_gen.elapsed());

    // --- Parse JSON strings into Value for insertion ---
    // We need Value for both PG JSONB COPY and ES Bulk
    println!("Parsing JSON strings...");
    let start_parse = Instant::now();
    let docs_value: Vec<Value> = docs_json_strings
        .iter()
        .map(|s| serde_json::from_str(s))
        .collect::<Result<Vec<_>, _>>()?;
    println!("JSON parsing took: {:?}", start_parse.elapsed());

    // --- Insertion (modified for JSONB COPY and ES Bulk) ---
    println!("Inserting data into PostgreSQL (JSONB)...");
    let start_pg_insert = Instant::now();
    insert_postgres(&pg_client, &docs_value).await?;
    println!("PostgreSQL JSONB insertion took: {:?}", start_pg_insert.elapsed());

    println!("Inserting data into Elasticsearch...");
    let start_es_insert = Instant::now();
    // Pass Value directly to ES insert function
    insert_elasticsearch_value(&es_client, &docs_value).await?;
    println!("Elasticsearch insertion took: {:?}", start_es_insert.elapsed());

    // --- Benchmarks (modified queries) ---
    // Define queries suitable for JSONB and ES structure
    let pg_queries = vec![
        // Tag containment ('@>') - Does tags array contain ["rust"]?
        ("tags @> 'rust'", json!(["rust"]).to_string()),
        // Attribute key existence ('?') - Does attributes object have key 'att1'?
        ("attr ? 'att1'", "att1".to_string()),
        // Nested attribute value ('->>') - Is attributes.att2.nested_key == 'com'?
        ("attr nested = 'com'", "com".to_string()), // We'll use ->> inside the query
        // Attribute value comparison ('>') - Is attributes.att0 > 500?
        ("attr att0 > 500", json!(500).to_string()),
        // Optional attribute existence ('?')
        ("attr ? 'att_opt_1'", "att_opt_1".to_string()),
        // Non-existent tag
        ("tags @> 'nonexistent'", json!(["nonexistent"]).to_string()),
    ];

    let es_queries = vec![
        // Match a specific tag (term query on keyword field)
        ("tags: rust", json!({"term": {"tags": "rust"}})),
        // Check for attribute existence (exists query)
        ("exists: attributes.att1", json!({"exists": {"field": "attributes.att1"}})),
        // Match nested attribute value (term query on keyword/text)
        // Assuming default mapping makes nested_key text/keyword
        ("attributes.att2.nested_key: com", json!({"term": {"attributes.att2.nested_key": "com"}})),
         // Range query on numeric attribute
        ("attributes.att0 > 500", json!({"range": {"attributes.att0": {"gt": 500}}})),
        // Check for optional attribute existence
        ("exists: attributes.att_opt_1", json!({"exists": {"field": "attributes.att_opt_1"}})),
        // Non-existent tag
        ("tags: nonexistent", json!({"term": {"tags": "nonexistent"}})),
    ];


    println!("\nRunning PostgreSQL JSONB benchmarks...");
    benchmark_postgres(&pg_client, &pg_queries).await?;

    println!("\nRunning Elasticsearch benchmarks...");
    benchmark_elasticsearch(&es_client, &es_queries).await?;

    println!("\nBenchmark finished.");
    Ok(())
}

// --- Connection Functions (remain the same) ---
async fn connect_postgres() -> Result<Client, BenchmarkError> {
    let db_url = env::var("DATABASE_URL")
        .map_err(|_| BenchmarkError::EnvVar("DATABASE_URL".to_string()))?;
    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("PostgreSQL connection error: {}", e);
        }
    });
    Ok(client)
}

// --- Setup Functions (Updated for JSONB and new ES Mapping) ---

async fn setup_postgres(client: &Client) -> Result<(), BenchmarkError> {
    // Create table with a single JSONB column
    // Add a GIN index for efficient JSONB operations
    client.batch_execute(&format!(
        r#"
        CREATE TABLE IF NOT EXISTS {PG_TABLE_NAME} (
            id SERIAL PRIMARY KEY, -- Keep ID for potential reference
            data JSONB NOT NULL
        );
        -- Create a GIN index on the JSONB column. This is crucial for performance.
        CREATE INDEX IF NOT EXISTS documents_data_gin_idx ON {PG_TABLE_NAME} USING GIN(data);
        CREATE INDEX IF NOT EXISTS documents_data_gin_json_idx ON {PG_TABLE_NAME} USING GIN (data jsonb_path_ops);
        CREATE INDEX IF NOT EXISTS documents_data_gin_jsonb_idx ON {PG_TABLE_NAME} USING GIN (data jsonb_ops);

        -- Optional: Index specific paths if needed for very specific query patterns
        CREATE INDEX IF NOT EXISTS documents_tags_gin_idx ON {PG_TABLE_NAME} USING GIN ((data -> 'tags'));
        CREATE INDEX IF NOT EXISTS documents_attr_gin_idx ON {PG_TABLE_NAME} USING GIN ((data -> 'attributes'));

        -- Optional: Clear table for a fresh benchmark run
        -- TRUNCATE TABLE {PG_TABLE_NAME} RESTART IDENTITY;
        "#, PG_TABLE_NAME=PG_TABLE_NAME)
    ).await?;
    println!("PostgreSQL table '{}' with JSONB column and GIN index checked/created.", PG_TABLE_NAME);
    Ok(())
}

async fn setup_elasticsearch(client: &Elasticsearch) -> Result<(), BenchmarkError> {
    let index_exists = client
        .indices()
        .exists(IndicesExistsParts::Index(&[ES_INDEX_NAME]))
        .send()
        .await?
        .status_code()
        .is_success();

    if !index_exists {
        println!("Creating Elasticsearch index '{}' with new mapping...", ES_INDEX_NAME);
        let create_response = client
            .indices()
            .create(IndicesCreateParts::Index(ES_INDEX_NAME))
            .body(json!({
                "mappings": {
                    "properties": {
                        "title": { "type": "text" },
                        "content": { "type": "text" },
                        "created_at": { "type": "date" },
                        // Index tags as keyword for exact matching, filtering, aggregations
                        "tags": { "type": "keyword" },
                        // Index attributes as an object. Dynamic mapping will handle sub-fields.
                        // For production, you might explicitly map known attributes
                        // (e.g., "att0": {"type": "integer"}) for better control.
                        "attributes": {
                            "type": "object",
                            // "enabled": true // default is true
                            "properties": {
                                "att0": { "type": "integer" }, // Explicitly map known numeric field
                                "att1": { "type": "text", "fields": { "keyword": { "type": "keyword", "ignore_above": 256 }}}, // Text + keyword
                                "att2": { "type": "object", "enabled": true }, // Allow dynamic mapping within att2
                                "att3": { "type": "keyword" } // Array of strings often best as keyword
                                // Optional attributes will be dynamically mapped
                            }
                        }
                    }
                }
            }))
            .send()
            .await?;

        if !create_response.status_code().is_success() {
            let response_body = create_response.text().await?;
            eprintln!("Failed to create index '{}': {}", ES_INDEX_NAME, response_body);
            return Err(BenchmarkError::EsBulkError(format!(
                "Failed to create index '{}'", ES_INDEX_NAME
            )));
        }
         println!("Elasticsearch index '{}' created.", ES_INDEX_NAME);
    } else {
        println!("Elasticsearch index '{}' already exists.", ES_INDEX_NAME);
        // Optional: Delete index for a fresh run
        // println!("Deleting existing Elasticsearch index '{}'...", ES_INDEX_NAME);
        // client.indices().delete(IndicesDeleteParts::Index(&[ES_INDEX_NAME])).send().await?;
        // setup_elasticsearch(client).await?; // Recurse to create it
    }
    Ok(())
}


// --- Insertion Functions (Updated for JSONB COPY and ES Value) ---

async fn insert_postgres(client: &Client, docs: &[Value]) -> Result<(), BenchmarkError> {
    // Use COPY BINARY for efficient bulk insertion of JSONB
    let copy_stmt = format!(
        // Copy into the 'data' column
        "COPY {PG_TABLE_NAME} (data) FROM STDIN (FORMAT BINARY)",
        PG_TABLE_NAME = PG_TABLE_NAME
    );

    let sink = client.copy_in(&copy_stmt).await?;

    // The type for the 'data' column is JSONB
    let types = &[Type::JSONB];
    let writer = BinaryCopyInWriter::new(sink, types);
    pin_mut!(writer);

    println!("Starting PostgreSQL COPY operation for {} documents...", docs.len());
    let pb = indicatif::ProgressBar::new(docs.len() as u64);
     pb.set_style(indicatif::ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));


    // Iterate through the serde_json::Value objects and write them
    // serde_json::Value implements ToSql for JSONB
    for doc_value in docs {
        // write expects a slice of references implementing ToSql
        writer.as_mut().write(&[doc_value]).await?;
        pb.inc(1);
    }

    // Finish the COPY operation
    writer.finish().await?;
    pb.finish_with_message("PostgreSQL COPY complete");

    Ok(())
}

// Keep the original insert_elasticsearch but rename it slightly
// This version works if you have Vec<Document>
// async fn insert_elasticsearch_struct(client: &Elasticsearch, docs: &[Document]) -> Result<(), BenchmarkError> { ... }

// New version accepting Vec<Value> directly
async fn insert_elasticsearch_value(client: &Elasticsearch, docs: &[Value]) -> Result<(), BenchmarkError> {
    let chunks = docs.chunks(BATCH_SIZE);

    println!("Inserting {} documents into Elasticsearch in batches of {}...", docs.len(), BATCH_SIZE);
    let pb = indicatif::ProgressBar::new(docs.len() as u64);
    pb.set_style(indicatif::ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
        .unwrap()
        .progress_chars("#>-"));

    for chunk in chunks {
        let mut operations: Vec<BulkOperation<Value>> = Vec::with_capacity(chunk.len());

        for doc_value in chunk {
            // Since we already have Value, just clone it for the operation
            // Use BulkOperation::index(doc_value.clone()).into()
            let op = BulkOperation::index(doc_value.clone()).into();
            operations.push(op);
            pb.inc(1);
        }

        if operations.is_empty() {
            continue;
        }

        let response = client
            .bulk(BulkParts::Index(ES_INDEX_NAME))
            .body(operations)
            .send()
            .await?;

        let status = response.status_code();

        if !status.is_success() {
            pb.finish_with_message(format!("Error during bulk insert (HTTP Status: {})!", status));
            let response_body_text = response.text().await?;
            eprintln!("Elasticsearch bulk insert failed with status {}: {}", status, response_body_text);
            return Err(BenchmarkError::EsBulkError(format!(
                "Bulk insert failed with status {} - Body: {}", status, response_body_text
            )));
        }

        let response_body = response.json::<Value>().await?;

        if let Some(true) = response_body.get("errors").and_then(|v| v.as_bool()) {
             pb.set_message(format!("Batch completed with item errors."));
             eprintln!("WARNING: Elasticsearch bulk operation reported errors for some items. Check response details.");
             // Consider logging response_body here for debugging errors
             // eprintln!("Bulk response with errors: {:?}", response_body);
        } else {
             pb.set_message(format!("Batch successful."));
        }
    }
    pb.finish_with_message("Elasticsearch insertion complete");

    // Force a refresh
    println!("Refreshing Elasticsearch index...");
    let refresh_start = Instant::now();
    client.indices().refresh(IndicesRefreshParts::Index(&[ES_INDEX_NAME])).send().await?;
    println!("Elasticsearch refresh took: {:?}", refresh_start.elapsed());

    Ok(())
}


// --- Benchmark Functions (Updated for JSONB and new ES Queries) ---

async fn benchmark_postgres(client: &Client, queries: &[(&str, String)]) -> Result<(), BenchmarkError> {
    println!("{:<25} | {:<10} | {:<15}", "Query Type", "Count", "Latency (ms)");
    println!("{:-<60}", "");

    let mut total_latency = Duration::ZERO;
    let mut total_rows_found = 0;
    let query_count = queries.len();

    // Prepare different statements for different JSONB operations
    // Note: Parameter types might need adjustment based on the operator
    let prep_tag_contains = client.prepare(&format!(
        "SELECT data ->> 'title' FROM {PG_TABLE_NAME} WHERE data -> 'tags' @> $1::jsonb LIMIT 10", PG_TABLE_NAME=PG_TABLE_NAME
    )).await?;
    let prep_attr_exists = client.prepare(&format!(
        "SELECT data ->> 'title' FROM {PG_TABLE_NAME} WHERE data -> 'attributes' ? $1 LIMIT 10", PG_TABLE_NAME=PG_TABLE_NAME
    )).await?;
    let prep_nested_attr_eq = client.prepare(&format!(
        "SELECT data ->> 'title' FROM {PG_TABLE_NAME} WHERE data -> 'attributes' -> 'att2' ->> 'nested_key' = $1 LIMIT 10", PG_TABLE_NAME=PG_TABLE_NAME
    )).await?;
     let prep_attr_compare_num = client.prepare(&format!(
        // Ensure casting for comparison. Use numeric for broader compatibility.
        "SELECT data ->> 'title' FROM {PG_TABLE_NAME} WHERE (data -> 'attributes' ->> 'att0')::numeric > 500::numeric LIMIT 10", PG_TABLE_NAME=PG_TABLE_NAME
    )).await?;


    for (query_desc, query_param_str) in queries {
        let start = Instant::now();
        let rows = match *query_desc {
            q if q.starts_with("tags @>") => {
                // Parameter needs to be a valid JSON string representing the array/value
                let param_jsonb: Value = serde_json::from_str(&query_param_str)
                    .map_err(|e| BenchmarkError::Conversion(format!("Invalid JSON for tag query: {} - {}", query_param_str, e)))?;
                client.query(&prep_tag_contains, &[&param_jsonb]).await?
            },
            q if q.starts_with("attr ?") => {
                // Parameter is the key name (string)
                client.query(&prep_attr_exists, &[&query_param_str]).await?
            },
            q if q.starts_with("attr nested =") => {
                 // Parameter is the value to compare against (string)
                client.query(&prep_nested_attr_eq, &[&query_param_str]).await?
            },
             q if q.starts_with("attr att0 >") => {
                // Parameter needs to be parsed as a number
                let param_num: f64 = query_param_str.parse()
                     .map_err(|e| BenchmarkError::Conversion(format!("Invalid number for comparison: {} - {}", query_param_str, e)))?;
                // Pass as f64, which ToSql handles for numeric
                client.query(&prep_attr_compare_num, &[]).await?
            }
            _ => {
                println!("WARN: Unsupported PG query description: {}", query_desc);
                vec![] // Return empty vec if query type not recognized
            }
        };
        let duration = start.elapsed();
        total_latency += duration;
        total_rows_found += rows.len();

        println!(
            "{:<25} | {:<10} | {:<15.4}",
            query_desc,
            rows.len(),
            duration.as_secs_f64() * 1000.0
        );
    }

    let avg_latency = if query_count > 0 { total_latency / query_count as u32 } else { Duration::ZERO };
    println!("{:-<60}", "");
    println!(
        "PostgreSQL Average Latency: {:.4}ms ({} queries, {} total results)",
        avg_latency.as_secs_f64() * 1000.0,
        query_count,
        total_rows_found
    );
    Ok(())
}

async fn benchmark_elasticsearch(client: &Elasticsearch, queries: &[(&str, Value)]) -> Result<(), BenchmarkError> {
    println!("{:<25} | {:<10} | {:<15}", "Query Type", "Count", "Latency (ms)");
    println!("{:-<60}", "");

    let mut total_latency = Duration::ZERO;
    let mut total_rows_found = 0;
    let query_count = queries.len();

    for (query_desc, es_query_json) in queries {
        let start = Instant::now();
        let response = client
            .search(SearchParts::Index(&[ES_INDEX_NAME]))
            .body(json!({
                "_source": ["title"], // Only fetch title
                "query": es_query_json, // Use the provided JSON query structure
                "size": 10
            }))
            .send()
            .await?;

        let duration = start.elapsed();
        total_latency += duration;

        // Check HTTP status before parsing JSON
        if !response.status_code().is_success() {
            let status = response.status_code();
             let error_body = response.text().await?;
             println!("WARN: Elasticsearch query failed for '{}' - Status: {}, Body: {}", query_desc, status, error_body);
             continue; // Skip this query
        }

        let response_body: Value = response.json().await?;
        let hits = response_body["hits"]["hits"].as_array().map_or(0, |h| h.len());
        total_rows_found += hits;

        println!(
            "{:<25} | {:<10} | {:<15.4}",
            query_desc,
            hits,
            duration.as_secs_f64() * 1000.0
        );
    }

    let avg_latency = if query_count > 0 { total_latency / query_count as u32 } else { Duration::ZERO };
    println!("{:-<60}", "");
    println!(
        "Elasticsearch Average Latency: {:.4}ms ({} queries, {} total results)",
        avg_latency.as_secs_f64() * 1000.0,
        query_count,
        total_rows_found
    );
    Ok(())
}

// Add indicatif to Cargo.toml if not already present:
// indicatif = { version = "0.17", features = ["tokio"] }
// Add fake = { version = "2.5", features = ["chrono"] } or similar
// Ensure chrono, serde, serde_json, tokio-postgres, elasticsearch, etc. are up-to-date.
