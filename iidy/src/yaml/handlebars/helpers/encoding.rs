//! Encoding helpers for handlebars templates
//!
//! Provides helpers for various encoding formats

use base64::{Engine as _, engine::general_purpose};
use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use std::path::Path;
use std::fs;
use sha2::{Digest, Sha256};

/// base64 helper - encodes string to base64
pub fn base64_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("base64 helper requires exactly one parameter")
    })?;

    let input_str = param
        .value()
        .as_str()
        .ok_or_else(|| handlebars::RenderError::new("base64 helper requires a string parameter"))?;

    let encoded = general_purpose::STANDARD.encode(input_str.as_bytes());
    out.write(&encoded)?;
    Ok(())
}

/// url helper - URL-encodes a string
pub fn url_encode_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("urlEncode helper requires exactly one parameter")
    })?;

    let input_str = param.value().as_str().ok_or_else(|| {
        handlebars::RenderError::new("urlEncode helper requires a string parameter")
    })?;

    let encoded = url::form_urlencoded::byte_serialize(input_str.as_bytes()).collect::<String>();
    out.write(&encoded)?;
    Ok(())
}

/// sha256 helper - computes SHA256 hash of a string
pub fn sha256_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("sha256 helper requires exactly one parameter")
    })?;

    let input_str = param
        .value()
        .as_str()
        .ok_or_else(|| handlebars::RenderError::new("sha256 helper requires a string parameter"))?;

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input_str.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    out.write(&hash)?;
    Ok(())
}

/// Calculate SHA256 hash of a file or directory 
/// 
/// For files: returns the SHA256 hash of the file contents
/// For directories: recursively finds all files, hashes each one, 
/// concatenates the hashes with commas, then hashes that result.
/// This matches the iidy-js behavior exactly.
fn calculate_filehash(path: &str) -> Result<String, handlebars::RenderError> {
    let path = Path::new(path);
    
    if !path.exists() {
        return Err(handlebars::RenderError::new(format!("Invalid path {} for filehash", path.display())));
    }
    
    if path.is_file() {
        // For files, read contents and hash directly
        let contents = fs::read(path).map_err(|e| {
            handlebars::RenderError::new(format!("Failed to read file {}: {}", path.display(), e))
        })?;
        
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        Ok(format!("{:x}", hasher.finalize()))
    } else if path.is_dir() {
        // For directories, find all files recursively, hash each, then hash the concatenated result
        let pattern = format!("{}/**/*", path.display());
        let paths = glob::glob(&pattern).map_err(|e| {
            handlebars::RenderError::new(format!("Failed to glob directory {}: {}", path.display(), e))
        })?;
        
        let mut file_hashes = Vec::new();
        for entry in paths {
            let file_path = entry.map_err(|e| {
                handlebars::RenderError::new(format!("Failed to process glob entry: {}", e))
            })?;
            
            // Skip directories (only process files)
            if file_path.is_file() {
                let contents = fs::read(&file_path).map_err(|e| {
                    handlebars::RenderError::new(format!("Failed to read file {}: {}", file_path.display(), e))
                })?;
                
                let mut hasher = Sha256::new();
                hasher.update(&contents);
                file_hashes.push(format!("{:x}", hasher.finalize()));
            }
        }
        
        // Join all file hashes with commas and hash the result
        let combined = file_hashes.join(",");
        let mut hasher = Sha256::new();
        hasher.update(combined.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    } else {
        Err(handlebars::RenderError::new(format!("Path {} is neither a file nor directory", path.display())))
    }
}

/// filehash helper - computes SHA256 hash of a file or directory contents (hex format)
pub fn filehash_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("filehash helper requires exactly one parameter")
    })?;

    let path_str = param
        .value()
        .as_str()
        .ok_or_else(|| handlebars::RenderError::new("filehash helper requires a string parameter"))?;

    let hash = calculate_filehash(path_str)?;
    out.write(&hash)?;
    Ok(())
}

/// filehashBase64 helper - computes SHA256 hash of a file or directory contents (base64 format)
pub fn filehash_base64_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("filehashBase64 helper requires exactly one parameter")
    })?;

    let path_str = param
        .value()
        .as_str()
        .ok_or_else(|| handlebars::RenderError::new("filehashBase64 helper requires a string parameter"))?;

    let hex_hash = calculate_filehash(path_str)?;
    
    // Convert hex to base64 (matching iidy-js behavior)
    let hex_bytes = hex::decode(&hex_hash).map_err(|e| {
        handlebars::RenderError::new(format!("Failed to decode hex hash: {}", e))
    })?;
    
    let base64_hash = general_purpose::STANDARD.encode(&hex_bytes);
    out.write(&base64_hash)?;
    Ok(())
}
