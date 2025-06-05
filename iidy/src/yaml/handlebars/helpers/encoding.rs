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