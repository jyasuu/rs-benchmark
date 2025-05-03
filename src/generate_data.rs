// src/generate_data.rs
use chrono::Utc;
use fake::{Fake, Faker}; // Use Faker for more diverse attribute values
use rand::Rng;
use serde_json::json; // Needed for creating the attributes object

const MAX_TAGS: usize = 5;
const MIN_TAGS: usize = 1;
const MAX_ATTR_KEYS: usize = 4; // Corresponds to att0, att1, att2, att3

pub async fn generate_documents(count: usize) -> Vec<String> {
    let mut rng = rand::thread_rng();
    let mut docs = Vec::with_capacity(count);

    println!("Generating {} documents with tags and attributes...", count);
    let pb = indicatif::ProgressBar::new(count as u64);
    pb.set_style(indicatif::ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    for i in 0..count {
        let title = fake::faker::lorem::zh_tw::Words(5..20).fake::<Vec<String>>().join(" ");
        let content = fake::faker::lorem::zh_tw::Paragraphs(5..10).fake::<Vec<String>>().join(" ");
        let created_at = Utc::now() - chrono::Duration::days(rng.gen_range(0..365));

        // Generate Tags
        let num_tags = rng.gen_range(MIN_TAGS..=MAX_TAGS);
        let tags: Vec<String> = (0..num_tags)
            // Generate more realistic-looking tags (e.g., single words)
            .map(|_| fake::faker::lorem::en::Word().fake::<String>().to_lowercase())
            .collect();

        // Generate Attributes (using serde_json::json! for structure)
        // Ensure diverse types as requested
        let attributes = json!({
            // att0: number (integer)
            "att0": rng.gen_range(0..1000),
            // att1: string
            "att1": fake::faker::company::en::Bs().fake::<String>(),
            // att2: nested object (can be simple or complex)
            "att2": {
                "nested_key": fake::faker::internet::en::DomainSuffix().fake::<String>(),
                "nested_bool": fake::faker::boolean::en::Boolean(50).fake::<bool>(), // 50% chance true/false
            },
            // att3: array of strings
            "att3": fake::faker::lorem::en::Words(2..5).fake::<Vec<String>>(),
            // Add potentially missing attribute sometimes for existence checks
            format!("att_opt_{}", i % 5) : if rng.gen_bool(0.7) { // ~70% chance this optional key exists
                Some(fake::faker::number::en::NumberWithFormat("###-##-####").fake::<String>())
            } else {
                None // This key won't be present in the JSON
            }
        });


        let doc = json!({
            "title": title,
            "content": content,
            "created_at": created_at.to_rfc3339(),
            "tags": tags,
            "attributes": attributes
        });

        docs.push(doc.to_string());
        pb.inc(1);
    }
    pb.finish_with_message("Document generation complete");

    docs
}
