//! String manipulation helpers for handlebars templates
//! 
//! Provides helpers for string operations like trimming and replacement

use handlebars::{Handlebars, Helper, Context, RenderContext, Output, HelperResult};

/// trim helper - removes leading and trailing whitespace from string
pub fn trim_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("trim helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("trim helper requires a string parameter"))?;
    
    out.write(string_value.trim())?;
    Ok(())
}

/// replace helper - replaces all occurrences of a search string with a replacement
pub fn replace_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires three parameters: string, search, replacement"))?
        .value();
    
    let search = h.param(1)
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires three parameters: string, search, replacement"))?
        .value();
    
    let replacement = h.param(2)
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires three parameters: string, search, replacement"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires a string as first parameter"))?;
    
    let search_str = search.as_str()
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires a string as second parameter"))?;
    
    let replacement_str = replacement.as_str()
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires a string as third parameter"))?;
    
    let replaced = string_value.replace(search_str, replacement_str);
    out.write(&replaced)?;
    Ok(())
}

/// substring helper - extracts a substring from the given string
pub fn substring_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("substring helper requires three parameters: string, start, length"))?
        .value();
    
    let start = h.param(1)
        .ok_or_else(|| handlebars::RenderError::new("substring helper requires three parameters: string, start, length"))?
        .value();
    
    let length = h.param(2)
        .ok_or_else(|| handlebars::RenderError::new("substring helper requires three parameters: string, start, length"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("substring helper requires a string as first parameter"))?;
    
    let start_idx = start.as_u64()
        .ok_or_else(|| handlebars::RenderError::new("substring helper requires a number as second parameter"))? as usize;
    
    let length_val = length.as_u64()
        .ok_or_else(|| handlebars::RenderError::new("substring helper requires a number as third parameter"))? as usize;
    
    let end_idx = (start_idx + length_val).min(string_value.len());
    
    if start_idx >= string_value.len() {
        out.write("")?;
    } else {
        let substring = &string_value[start_idx..end_idx];
        out.write(substring)?;
    }
    
    Ok(())
}

/// length helper - gets the length of a string or array
pub fn length_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("length helper requires one parameter"))?
        .value();
    
    let length = match value {
        serde_json::Value::String(s) => s.len(),
        serde_json::Value::Array(arr) => arr.len(),
        serde_json::Value::Object(obj) => obj.len(),
        _ => return Err(handlebars::RenderError::new("length helper can only be used on strings, arrays, or objects")),
    };
    
    out.write(&length.to_string())?;
    Ok(())
}

/// pad helper - pads a string to a specific length
pub fn pad_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("pad helper requires at least two parameters: string, length, [padChar]"))?
        .value();
    
    let target_length = h.param(1)
        .ok_or_else(|| handlebars::RenderError::new("pad helper requires at least two parameters: string, length, [padChar]"))?
        .value();
    
    let pad_char = h.param(2)
        .and_then(|p| p.value().as_str())
        .unwrap_or(" ");
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("pad helper requires a string as first parameter"))?;
    
    let target_len = target_length.as_u64()
        .ok_or_else(|| handlebars::RenderError::new("pad helper requires a number as second parameter"))? as usize;
    
    if string_value.len() >= target_len {
        out.write(string_value)?;
    } else {
        let pad_count = target_len - string_value.len();
        let padding = pad_char.repeat(pad_count);
        out.write(&format!("{}{}", string_value, padding))?;
    }
    
    Ok(())
}

/// concat helper - concatenates multiple strings
pub fn concat_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let mut result = String::new();
    
    for param in h.params() {
        match param.value() {
            serde_json::Value::String(s) => result.push_str(s),
            serde_json::Value::Number(n) => result.push_str(&n.to_string()),
            serde_json::Value::Bool(b) => result.push_str(&b.to_string()),
            _ => return Err(handlebars::RenderError::new("concat helper only supports strings, numbers, and booleans")),
        }
    }
    
    out.write(&result)?;
    Ok(())
}