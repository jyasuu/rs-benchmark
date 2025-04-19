use std::time::{Duration, Instant};
use std::env;
use dotenv::dotenv;
use elasticsearch::{Elasticsearch, Error as EsError, http::transport::{Transport, TransportBuilder}, SearchParts, BulkParts, indices::IndicesExistsParts, indices::IndicesCreateParts};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use tokio_postgres::{Client, NoTls, Error as PgError};
use url::Url;

use tokio_postgres::types::Type; // <-- Add this import
use futures_util::pin_mut;       // <-- Add this import
use tokio_postgres::binary_copy::BinaryCopyInWriter; // <-- Add this import


// Declare the module
mod generate_data;

const BATCH_SIZE: usize = 500; // For bulk inserts
const ES_INDEX_NAME: &str = "documents";
const PG_TABLE_NAME: &str = "documents";

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
}

// Simple struct to deserialize JSON data for insertion
#[derive(Serialize, Deserialize, Debug)]
struct Document {
    title: String,
    content: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok(); // Load .env file

    println!("Starting benchmark...");

    // Initialize connections
    println!("Connecting to databases...");
    let pg_client = connect_postgres().await?;
    // let es_client = connect_elasticsearch().await?;
    let transport = Transport::single_node("http://localhost:9200")?;
    let es_client = Elasticsearch::new(transport);
    // let es_client = Elasticsearch::default();
    println!("Connections established.");

    // Setup databases (create table/index if they don't exist)
    println!("Setting up database schemas...");
    setup_postgres(&pg_client).await?;
    setup_elasticsearch(&es_client).await?;
    println!("Schemas ready.");

    // Generate test data
    let data_count = 10;
    println!("Generating {} documents...", data_count);
    let start_gen = Instant::now();
    let docs_json = generate_data::generate_documents(data_count).await;
    println!("Data generation took: {:?}", start_gen.elapsed());

    // Parse JSON strings into Document structs
    let docs: Vec<Document> = docs_json
        .iter()
        .map(|s| serde_json::from_str(s))
        .collect::<Result<Vec<_>, _>>()?;

    // Insert data into both databases
    println!("Inserting data into PostgreSQL...");
    let start_pg_insert = Instant::now();
    insert_postgres(&pg_client, &docs).await?;
    println!("PostgreSQL insertion took: {:?}", start_pg_insert.elapsed());

    println!("Inserting data into Elasticsearch...");
    let start_es_insert = Instant::now();
    // insert_elasticsearch(&es_client, &docs).await?;
    println!("Elasticsearch insertion took: {:?}", start_es_insert.elapsed());

    // Run benchmarks
    let queries = vec![
        "database performance",
        "search engine",
        "distributed systems",
        "rust programming",
        "benchmark results",
        "lorem ipsum dolor", // Add more generic terms likely present
        "quick brown fox",   // Add terms unlikely to be present
    ];

    println!("\nRunning PostgreSQL benchmarks...");
    benchmark_postgres(&pg_client, &queries).await?;

    println!("\nRunning Elasticsearch benchmarks...");
    benchmark_elasticsearch(&es_client, &queries).await?;

    println!("\nBenchmark finished.");
    Ok(())
}

// --- Connection Functions ---

async fn connect_postgres() -> Result<Client, BenchmarkError> {
    let db_url = env::var("DATABASE_URL")
        .map_err(|_| BenchmarkError::EnvVar("DATABASE_URL".to_string()))?;
    let (client, connection) = tokio_postgres::connect(&db_url, NoTls).await?;

    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("PostgreSQL connection error: {}", e);
        }
    });

    Ok(client)
}

// async fn connect_elasticsearch() -> Result<Elasticsearch, BenchmarkError> {
//     let es_url = env::var("ELASTICSEARCH_URL")
//         .map_err(|_| BenchmarkError::EnvVar("ELASTICSEARCH_URL".to_string()))?;
//     let url = Url::parse(&es_url)?;

//     let transport = TransportBuilder::new(Transport::single_node(&es_url)?)
//         // .auth(...) // Add authentication if needed
//         .build()?;
//     Ok(Elasticsearch::new(transport))
// }

// --- Setup Functions ---

async fn setup_postgres(client: &Client) -> Result<(), BenchmarkError> {
    // Use IF NOT EXISTS to avoid errors on subsequent runs
    // Create a tsvector column and index for efficient FTS
    client.batch_execute(&format!(
        r#"
        CREATE TABLE IF NOT EXISTS {PG_TABLE_NAME} (
            id SERIAL PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        );
        -- Add tsvector column if it doesn't exist
        DO $$
        BEGIN
            ALTER TABLE {PG_TABLE_NAME} ADD COLUMN IF NOT EXISTS fts_doc tsvector;
        EXCEPTION
            WHEN duplicate_column THEN -- Handle potential race condition if run concurrently
                RAISE NOTICE 'Column fts_doc already exists in {PG_TABLE_NAME}.';
        END;
        $$;
        -- Update existing rows where fts_doc might be null (e.g., if table existed before)
        UPDATE {PG_TABLE_NAME} SET fts_doc = to_tsvector('english', title || ' ' || content) WHERE fts_doc IS NULL;

        -- Create the GIN index if it doesn't exist
        CREATE INDEX IF NOT EXISTS documents_fts_idx ON {PG_TABLE_NAME} USING GIN(fts_doc);

        -- Optional: Clear table for a fresh benchmark run
        -- TRUNCATE TABLE {PG_TABLE_NAME};
        "#, PG_TABLE_NAME=PG_TABLE_NAME)
    ).await?;
    println!("PostgreSQL table '{}' and FTS index checked/created.", PG_TABLE_NAME);
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
        println!("Creating Elasticsearch index '{}'...", ES_INDEX_NAME);
        let create_response = client
            .indices()
            .create(IndicesCreateParts::Index(ES_INDEX_NAME))
            .body(json!({
                "mappings": {
                    "properties": {
                        "title": { "type": "text" },
                        "content": { "type": "text" },
                        "created_at": { "type": "date" }
                    }
                }
            }))
            .send()
            .await?;

        if !create_response.status_code().is_success() {
             // Read the response body for more details on failure
            let response_body = create_response.text().await?;
            eprintln!("Failed to create index: {}", response_body);
            return Err(BenchmarkError::EsBulkError(format!(
                "Failed to create index '{}'", ES_INDEX_NAME
            )));
        }
         println!("Elasticsearch index '{}' created.", ES_INDEX_NAME);
    } else {
        println!("Elasticsearch index '{}' already exists.", ES_INDEX_NAME);
        // Optional: Delete and recreate index for a fresh run
        // client.indices().delete(IndicesDeleteParts::Index(&[ES_INDEX_NAME])).send().await?;
        // setup_elasticsearch(client).await?; // Recurse to create it
    }
    Ok(())
}


// --- Insertion Functions ---
async fn insert_postgres(client: &Client, docs: &[Document]) -> Result<(), BenchmarkError> {
    // Use COPY for efficient bulk insertion
    let copy_stmt = format!(
        "COPY {PG_TABLE_NAME} (title, content, created_at) FROM STDIN (FORMAT BINARY)",
        PG_TABLE_NAME=PG_TABLE_NAME
    );

    // 1. Get the sink for the COPY IN operation
    let sink = client.copy_in(&copy_stmt).await?;

    // 2. Define the data types of the columns being copied IN ORDER
    let types = &[Type::TEXT, Type::TEXT, Type::TIMESTAMPTZ];

    // 3. Create the BinaryCopyInWriter helper
    //    It takes the sink and the expected column types.
    let writer = BinaryCopyInWriter::new(sink, types);

    // 4. Pin the writer to the stack for use with async/await.
    //    The `write` method requires `Pin<&mut Self>`.
    pin_mut!(writer);

    // 5. Iterate through the documents and write each one using the writer.
    //    The `write` method takes a slice of references to values that implement `ToSql`.
    //    `String` implements `ToSql` for `TEXT`.
    //    `chrono::DateTime<Utc>` implements `ToSql` for `TIMESTAMPTZ`.
    for doc in docs {
        writer
            .as_mut()
            .write(&[&doc.title, &doc.content, &doc.created_at])
            .await?; // Propagate potential PgError
    }

    // 6. Finish the COPY operation. This flushes any remaining buffered data
    //    and signals completion to the database.
    writer.finish().await?; // Propagate potential PgError

    // --- FTS Vector Update (remains the same) ---
    // This still needs to be done after the data is successfully copied.
    println!("Updating FTS vectors in PostgreSQL...");
    let update_start = Instant::now();
    let updated_rows = client.execute(
        &format!("UPDATE {PG_TABLE_NAME} SET fts_doc = to_tsvector('english', title || ' ' || content) WHERE fts_doc IS NULL", PG_TABLE_NAME=PG_TABLE_NAME),
        &[]
    ).await?;
    println!("FTS vector update took: {:?}, updated {} potential rows", update_start.elapsed(), updated_rows);

    Ok(())
}

// async fn insert_elasticsearch(client: &Elasticsearch, docs: &[Document]) -> Result<(), BenchmarkError> {
//     let chunks = docs.chunks(BATCH_SIZE); // Process in batches

//     for chunk in chunks {
//         let mut body: Vec<u8> = Vec::new();
//         for doc in chunk {
//             // Add the action line (index into our specific index)
//             let action = json!({ "index": { "_index": ES_INDEX_NAME } });
//             body.extend_from_slice(serde_json::to_string(&action)?.as_bytes());
//             body.push(b'\n'); // Newline delimiter

//             // Add the document source
//             body.extend_from_slice(serde_json::to_string(doc)?.as_bytes());
//             body.push(b'\n'); // Newline delimiter
//         }

//         let response = client
//             .bulk(BulkParts::Index(ES_INDEX_NAME))
//             .body(body)
//             .send()
//             .await?;

//         if !response.status_code().is_success() {
//             let response_body = response.text().await?;
//             eprintln!("Elasticsearch bulk insert failed: {}", response_body);
//             return Err(BenchmarkError::EsBulkError("Bulk insert failed".to_string()));
//         }

//         // Check response body for item-level errors (optional but recommended)
//         let response_json: serde_json::Value = response.json().await?;
//         if response_json["errors"].as_bool().unwrap_or(false) {
//             eprintln!("Errors occurred during Elasticsearch bulk insert:");
//             if let Some(items) = response_json["items"].as_array() {
//                 for item in items {
//                     if let Some(op_type) = item.as_object().and_then(|o| o.keys().next()) {
//                         if let Some(error) = item[op_type]["error"].as_object() {
//                              eprintln!("  Error: {:?}", error);
//                         }
//                     }
//                 }
//             }
//              return Err(BenchmarkError::EsBulkError("Errors reported in bulk response items".to_string()));
//         }
//     }

//     // Force a refresh for consistent search results immediately after indexing
//     println!("Refreshing Elasticsearch index...");
//     let refresh_start = Instant::now();
//     client.indices().refresh(elasticsearch::indices::IndicesRefreshParts::Index(&[ES_INDEX_NAME])).send().await?;
//     println!("Elasticsearch refresh took: {:?}", refresh_start.elapsed());


//     Ok(())
// }


// --- Benchmark Functions (Updated PG Query) ---

async fn benchmark_postgres(client: &Client, queries: &[&str]) -> Result<(), BenchmarkError> {
    println!("{:<25} | {:<10} | {:<15}", "Query", "Count", "Latency (ms)");
    println!("{:-<60}", ""); // Separator line

    let mut total_latency = Duration::ZERO;
    let mut total_rows_found = 0;
    let query_count = queries.len();

    // Use the precomputed tsvector column and GIN index
    let statement = client.prepare(
        &format!(r#"
            SELECT id, title, ts_rank_cd(fts_doc, plainto_tsquery('english', $1)) as rank
            FROM {PG_TABLE_NAME}
            WHERE fts_doc @@ plainto_tsquery('english', $1)
            ORDER BY rank DESC
            LIMIT 10
        "#, PG_TABLE_NAME=PG_TABLE_NAME)
    ).await?;

    for query in queries {
        let start = Instant::now();
        let rows = client.query(&statement, &[&query]).await?;
        let duration = start.elapsed();
        total_latency += duration;
        total_rows_found += rows.len();

        println!(
            "{:<25} | {:<10} | {:<15.4}",
            query,
            rows.len(),
            duration.as_secs_f64() * 1000.0
        );
        // Optional: Print found titles
        // for row in rows {
        //     let title: &str = row.get("title");
        //     println!("  - {}", title);
        // }
    }

    let avg_latency = if query_count > 0 { total_latency / query_count as u32 } else { Duration::ZERO };
    println!("{:-<60}", ""); // Separator line
    println!(
        "PostgreSQL Average Latency: {:.4}ms ({} queries, {} total results)",
        avg_latency.as_secs_f64() * 1000.0,
        query_count,
        total_rows_found
    );
    Ok(())
}

async fn benchmark_elasticsearch(client: &Elasticsearch, queries: &[&str]) -> Result<(), BenchmarkError> {
    println!("{:<25} | {:<10} | {:<15}", "Query", "Count", "Latency (ms)");
    println!("{:-<60}", ""); // Separator line

    let mut total_latency = Duration::ZERO;
    let mut total_rows_found = 0;
    let query_count = queries.len();

    for query in queries {
        let start = Instant::now();
        let response = client
            .search(SearchParts::Index(&[ES_INDEX_NAME]))
            .body(json!({
                "_source": ["title"], // Only fetch title if needed, otherwise false
                "query": {
                    "multi_match": {
                        "query": query,
                        "fields": ["title", "content"]
                    }
                },
                "size": 10
            }))
            .send()
            .await?;

        let duration = start.elapsed();
        total_latency += duration;

        let response_body: serde_json::Value = response.json().await?;
        let hits = response_body["hits"]["hits"].as_array().map_or(0, |h| h.len());
        total_rows_found += hits;

        println!(
            "{:<25} | {:<10} | {:<15.4}",
            query,
            hits,
            duration.as_secs_f64() * 1000.0
        );

        // Optional: Print found titles
        // if let Some(hits_array) = response_body["hits"]["hits"].as_array() {
        //     for hit in hits_array {
        //         if let Some(title) = hit["_source"]["title"].as_str() {
        //             println!("  - {}", title);
        //         }
        //     }
        // }
    }

    let avg_latency = if query_count > 0 { total_latency / query_count as u32 } else { Duration::ZERO };
    println!("{:-<60}", ""); // Separator line
    println!(
        "Elasticsearch Average Latency: {:.4}ms ({} queries, {} total results)",
        avg_latency.as_secs_f64() * 1000.0,
        query_count,
        total_rows_found
    );
    Ok(())
}

