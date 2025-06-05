//! String case transformation helpers for handlebars templates
//! 
//! Provides helpers for converting between different string cases

use handlebars::{Handlebars, Helper, Context, RenderContext, Output, HelperResult};

/// toLowerCase helper - converts string to lowercase
pub fn to_lower_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toLowerCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("toLowerCase helper requires a string parameter"))?;
    
    out.write(&string_value.to_lowercase())?;
    Ok(())
}

/// toUpperCase helper - converts string to uppercase
pub fn to_upper_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toUpperCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("toUpperCase helper requires a string parameter"))?;
    
    out.write(&string_value.to_uppercase())?;
    Ok(())
}

/// titleize helper - converts string to title case (capitalizes first letter of each word)
pub fn titleize_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("titleize helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("titleize helper requires a string parameter"))?;
    
    let titleized = string_value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    
    out.write(&titleized)?;
    Ok(())
}

/// camelCase helper - converts string to camelCase
pub fn camel_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("camelCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("camelCase helper requires a string parameter"))?;
    
    let camel_cased = string_value
        .split_whitespace()
        .enumerate()
        .map(|(i, word)| {
            if i == 0 {
                word.to_lowercase()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("");
    
    out.write(&camel_cased)?;
    Ok(())
}

/// snakeCase helper - converts string to snake_case
pub fn snake_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("snakeCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("snakeCase helper requires a string parameter"))?;
    
    let snake_cased = string_value
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join("_");
    
    out.write(&snake_cased)?;
    Ok(())
}

/// kebabCase helper - converts string to kebab-case
pub fn kebab_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("kebabCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("kebabCase helper requires a string parameter"))?;
    
    let kebab_cased = string_value
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join("-");
    
    out.write(&kebab_cased)?;
    Ok(())
}

/// capitalize helper - capitalizes the first character of the string
pub fn capitalize_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("capitalize helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("capitalize helper requires a string parameter"))?;
    
    let mut chars = string_value.chars();
    let capitalized = match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    };
    
    out.write(&capitalized)?;
    Ok(())
}