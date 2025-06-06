//! Import loaders for various data sources
//! 
//! This module provides a collection of loaders for different import types
//! including files, environment variables, git commands, random values,
//! HTTP endpoints, and AWS services.

pub mod utils;
pub mod file;
pub mod env;
pub mod git;
pub mod random;
pub mod http;

// Re-export the main loader functions
pub use file::{load_file_import, load_filehash_import};
pub use env::load_env_import;
pub use git::load_git_import;
pub use random::load_random_import;
pub use http::load_http_import;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;

use crate::yaml::imports::{ImportLoader, ImportData, ImportType};

/// Production import loader that routes to specific loader implementations
pub struct ProductionImportLoader {
    aws_config: Option<aws_config::SdkConfig>,
}

impl ProductionImportLoader {
    pub fn new() -> Self {
        Self { aws_config: None }
    }
    
    /// Configure AWS SDK for AWS-based imports
    pub fn with_aws_config(mut self, config: aws_config::SdkConfig) -> Self {
        self.aws_config = Some(config);
        self
    }
}

impl Default for ProductionImportLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ImportLoader for ProductionImportLoader {
    async fn load(&self, location: &str, base_location: &str) -> Result<ImportData> {
        let import_type = ImportType::from_location(location, base_location)?;
        
        match import_type {
            ImportType::File => load_file_import(location, base_location).await,
            ImportType::Env => load_env_import(location, base_location).await,
            ImportType::Git => load_git_import(location, base_location).await,
            ImportType::Random => load_random_import(location, base_location).await,
            ImportType::Filehash => load_filehash_import(location, base_location, false).await,
            ImportType::FilehashBase64 => load_filehash_import(location, base_location, true).await,
            ImportType::Http => {
                let client = Client::new();
                load_http_import(location, base_location, &client).await
            },
            ImportType::S3 => {
                // Placeholder for S3 loader - implementation from original file
                Err(anyhow!("S3 imports not yet implemented in new module structure"))
            },
            ImportType::Cfn => {
                // Placeholder for CloudFormation loader - implementation from original file
                Err(anyhow!("CloudFormation imports not yet implemented in new module structure"))
            },
            ImportType::Ssm => {
                // Placeholder for SSM loader - implementation from original file
                Err(anyhow!("SSM parameter imports not yet implemented in new module structure"))
            },
            ImportType::SsmPath => {
                // Placeholder for SSM path loader - implementation from original file
                Err(anyhow!("SSM parameter path imports not yet implemented in new module structure"))
            },
        }
    }
}