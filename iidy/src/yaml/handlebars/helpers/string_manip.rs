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