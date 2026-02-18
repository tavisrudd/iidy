//! Object access helpers for handlebars templates
//!
//! Provides helpers for accessing object properties and array elements

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};
use serde_json::Value;

/// lookup helper - looks up a property in an object or element in an array
pub fn lookup_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let object = h
        .param(0)
        .ok_or_else(|| {
            handlebars::RenderError::new("lookup helper requires two parameters: object and key")
        })?
        .value();

    let key = h
        .param(1)
        .ok_or_else(|| {
            handlebars::RenderError::new("lookup helper requires two parameters: object and key")
        })?
        .value();

    let key_str = key
        .as_str()
        .ok_or_else(|| handlebars::RenderError::new("lookup helper requires key to be a string"))?;

    match object {
        Value::Object(obj) => {
            if let Some(value) = obj.get(key_str) {
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "".to_string(),
                    _ => serde_json::to_string(value).map_err(|e| {
                        handlebars::RenderError::new(&format!(
                            "Failed to serialize lookup result: {}",
                            e
                        ))
                    })?,
                };
                out.write(&value_str)?;
            }
            // If key not found, output nothing (handlebars convention)
        }
        Value::Array(arr) => {
            if let Ok(index) = key_str.parse::<usize>() {
                if let Some(value) = arr.get(index) {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => "".to_string(),
                        _ => serde_json::to_string(value).map_err(|e| {
                            handlebars::RenderError::new(&format!(
                                "Failed to serialize lookup result: {}",
                                e
                            ))
                        })?,
                    };
                    out.write(&value_str)?;
                }
            }
        }
        _ => {
            return Err(handlebars::RenderError::new(
                "lookup helper requires first parameter to be an object or array",
            ));
        }
    }

    Ok(())
}
