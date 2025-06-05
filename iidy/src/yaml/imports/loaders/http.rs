//! HTTP/HTTPS import loader
//! 
//! Provides functionality for fetching content from HTTP/HTTPS URLs

use anyhow::Result;

use crate::yaml::imports::{ImportData, ImportType};
use super::utils::resolve_doc_from_import_data;

/// Load an HTTP import
pub async fn load_http_import(location: &str, _base_location: &str, client: &reqwest::Client) -> Result<ImportData> {
    let data = client.get(location).send().await?
        .text().await?;
    
    let doc = resolve_doc_from_import_data(&data, location)?;

    Ok(ImportData {
        import_type: ImportType::Http,
        resolved_location: location.to_string(),
        data,
        doc,
    })
}