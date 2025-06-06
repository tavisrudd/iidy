//! Encoding helpers for handlebars templates
//! 
//! Provides helpers for various encoding formats

use handlebars::{Handlebars, Helper, Context, RenderContext, Output, HelperResult};
use base64::{Engine as _, engine::general_purpose};

/// base64 helper - encodes string to base64
pub fn base64_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("base64 helper requires exactly one parameter"))?;
    
    let input_str = param.value().as_str()
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
    let param = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("urlEncode helper requires exactly one parameter"))?;
    
    let input_str = param.value().as_str()
        .ok_or_else(|| handlebars::RenderError::new("urlEncode helper requires a string parameter"))?;
    
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
    let param = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("sha256 helper requires exactly one parameter"))?;
    
    let input_str = param.value().as_str()
        .ok_or_else(|| handlebars::RenderError::new("sha256 helper requires a string parameter"))?;
    
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(input_str.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    
    out.write(&hash)?;
    Ok(())
}