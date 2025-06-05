//! Random value generation import loader
//! 
//! Provides functionality for generating random names and numbers

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::yaml::imports::{ImportData, ImportType};

/// Load a random import (dashed-name, name, int)
pub async fn load_random_import(location: &str, base_location: &str) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0] != "random" {
        return Err(anyhow!("Invalid random import format: {}", location));
    }

    let random_type = parts[1];
    let data = match random_type {
        "dashed-name" => generate_dashed_name(),
        "name" => generate_name(),
        "int" => generate_random_int(),
        _ => return Err(anyhow!("Invalid random type in {} at {}", location, base_location)),
    };

    Ok(ImportData {
        import_type: ImportType::Random,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Generate a dashed name for random imports
fn generate_dashed_name() -> String {
    use rand::Rng;
    let adjectives = ["red", "blue", "green", "happy", "clever", "brave", "swift", "mighty"];
    let nouns = ["cat", "dog", "bird", "fish", "lion", "eagle", "shark", "tiger"];
    
    let mut rng = rand::thread_rng();
    let adj = adjectives[rng.gen_range(0..adjectives.len())];
    let noun = nouns[rng.gen_range(0..nouns.len())];
    
    format!("{}-{}", adj, noun)
}

/// Generate a name (no dashes) for random imports
fn generate_name() -> String {
    generate_dashed_name().replace('-', "")
}

/// Generate a random integer for random imports
fn generate_random_int() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen_range(1..1000).to_string()
}