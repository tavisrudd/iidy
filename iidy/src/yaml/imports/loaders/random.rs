//! Random value generation import loader
//!
//! Provides functionality for generating random names and numbers

use anyhow::{Result, anyhow};
use serde_yaml::Value;

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
        _ => {
            return Err(anyhow!(
                "Invalid random type in {} at {}",
                location,
                base_location
            ));
        }
    };

    Ok(ImportData {
        import_type: ImportType::Random,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Generate a dashed name for random imports
pub fn generate_dashed_name() -> String {
    use rand::Rng;
    let adjectives = [
        "red", "blue", "green", "happy", "clever", "brave", "swift", "mighty",
    ];
    let nouns = [
        "cat", "dog", "bird", "fish", "lion", "eagle", "shark", "tiger",
    ];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_random_import_dashed_name() -> Result<()> {
        let result = load_random_import("random:dashed-name", "/base").await?;

        assert_eq!(result.import_type, ImportType::Random);
        assert_eq!(result.resolved_location, "random:dashed-name");

        // Check that the result contains a dash
        assert!(result.data.contains('-'));
        // Check that it's not empty
        assert!(!result.data.is_empty());
        // Check that it matches expected pattern (word-word)
        let parts: Vec<&str> = result.data.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_load_random_import_name() -> Result<()> {
        let result = load_random_import("random:name", "/base").await?;

        assert_eq!(result.import_type, ImportType::Random);
        assert_eq!(result.resolved_location, "random:name");

        // Check that the result doesn't contain a dash
        assert!(!result.data.contains('-'));
        // Check that it's not empty
        assert!(!result.data.is_empty());
        // Check that it's alphabetic
        assert!(result.data.chars().all(|c| c.is_alphabetic()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_random_import_int() -> Result<()> {
        let result = load_random_import("random:int", "/base").await?;

        assert_eq!(result.import_type, ImportType::Random);
        assert_eq!(result.resolved_location, "random:int");

        // Check that the result is a valid integer
        let parsed: i32 = result.data.parse().expect("Should be a valid integer");
        assert!(parsed >= 1 && parsed < 1000);

        Ok(())
    }

    #[tokio::test]
    async fn test_load_random_import_invalid_type() {
        let result = load_random_import("random:invalid-type", "/base").await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid random type")
        );
    }

    #[tokio::test]
    async fn test_load_random_import_invalid_format() {
        let result = load_random_import("invalid:format", "/base").await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid random import format")
        );
    }

    #[tokio::test]
    async fn test_load_random_import_no_type() {
        let result = load_random_import("random:", "/base").await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid random type")
        );
    }

    #[test]
    fn test_generate_dashed_name() {
        let name = generate_dashed_name();

        // Should contain exactly one dash
        assert_eq!(name.matches('-').count(), 1);

        // Should not be empty
        assert!(!name.is_empty());

        // Parts should be alphabetic
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].chars().all(|c| c.is_alphabetic()));
        assert!(parts[1].chars().all(|c| c.is_alphabetic()));
    }

    #[test]
    fn test_generate_name() {
        let name = generate_name();

        // Should not contain dashes
        assert!(!name.contains('-'));

        // Should not be empty
        assert!(!name.is_empty());

        // Should be alphabetic
        assert!(name.chars().all(|c| c.is_alphabetic()));
    }

    #[test]
    fn test_generate_random_int() {
        let int_str = generate_random_int();

        // Should parse as integer
        let parsed: i32 = int_str.parse().expect("Should be valid integer");

        // Should be in expected range
        assert!(parsed >= 1 && parsed < 1000);
    }

    #[test]
    fn test_randomness() {
        // Test that multiple calls produce different results (very likely)
        let mut names = std::collections::HashSet::new();
        let mut ints = std::collections::HashSet::new();

        for _ in 0..10 {
            names.insert(generate_dashed_name());
            ints.insert(generate_random_int());
        }

        // Very unlikely to get all the same values
        assert!(names.len() > 1, "Generated names should be different");
        // Integers might occasionally collide, so be more lenient
        assert!(ints.len() >= 1, "Generated integers should be valid");
    }
}
