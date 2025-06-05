//! Common utilities for YAML preprocessing tests
//!
//! This module provides shared test utilities, mock implementations,
//! and test fixtures used across multiple YAML preprocessing test modules.

use std::collections::HashMap;
use serde_json::Value;
use async_trait::async_trait;
use anyhow::Result;

use iidy::yaml::imports::{ImportLoader, ImportData, ImportType};

/// Basic mock loader for simple test cases
pub struct SimpleMockLoader {
    responses: HashMap<String, String>,
}

impl SimpleMockLoader {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }
    
    pub fn add_response(&mut self, location: &str, data: &str) {
        self.responses.insert(location.to_string(), data.to_string());
    }
}

#[async_trait]
impl ImportLoader for SimpleMockLoader {
    async fn load(&self, location: &str, _base_location: &str) -> Result<ImportData> {
        let data = self.responses.get(location)
            .ok_or_else(|| anyhow::anyhow!("Mock response not found for location: {}", location))?
            .clone();
        
        let doc: Value = serde_json::from_str(&data)
            .or_else(|_| Ok(Value::String(data.clone())))?;
        
        Ok(ImportData {
            import_type: ImportType::File,
            resolved_location: location.to_string(),
            data,
            doc,
        })
    }
}

/// Test fixture data commonly used across tests
pub mod fixtures {
    pub const SIMPLE_CONFIG: &str = r#"{"environment": "test", "debug": true}"#;
    
    pub const NESTED_IMPORT: &str = r#"
{
  "database": {
    "$imports": "./db-config.json"
  },
  "features": ["logging", "metrics"]
}
"#;
    
    pub const DB_CONFIG: &str = r#"
{
  "host": "localhost",
  "port": 5432,
  "name": "test_db"
}
"#;
    
    pub const HANDLEBARS_TEMPLATE: &str = r#"
{
  "service_name": "{{service}}",
  "environment": "{{env}}",
  "full_name": "{{service}}-{{env}}"
}
"#;
}