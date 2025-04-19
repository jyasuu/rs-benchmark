// src/generate_data.rs
// (Your existing code is fine, ensure imports match Cargo.toml)
use chrono::Utc;
// Adjust this import based on your actual faker crate:
// Option 1: If using 'faker' crate directly (less common now)
// use faker::Faker;
// use faker::locales::EN;
// Option 2: If using 'faker-rs' crate
use fake::Fake;
use rand::Rng;

pub async fn generate_documents(count: usize) -> Vec<String> {
    let mut rng = rand::thread_rng();
    // Adjust Faker instantiation if needed based on crate

    let mut docs = Vec::with_capacity(count);

    println!("Generating {} documents using Faker...", count);
    let pb = indicatif::ProgressBar::new(count as u64); // Add progress bar
    pb.set_style(indicatif::ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));


    for _ in 0..count {
        // Adjust fake data generation calls if needed based on crate API
        let title= fake::faker::lorem::zh_tw::Words(5..20).fake::<Vec<String>>().join(" ");
        let content = fake::faker::lorem::zh_tw::Paragraphs(5..10).fake::<Vec<String>>().join(" "); 
        let created_at = Utc::now() - chrono::Duration::days(rng.gen_range(0..365));

        // Escape JSON string literals properly
        let doc = serde_json::json!({
            "title": title,
            "content": content,
            "created_at": created_at.to_rfc3339()
        });

        docs.push(doc.to_string());
        pb.inc(1);
    }
    pb.finish_with_message("Document generation complete");


    docs
}

// Add indicatif to Cargo.toml if using the progress bar:
// indicatif = { version = "0.17", features = ["tokio"] }
