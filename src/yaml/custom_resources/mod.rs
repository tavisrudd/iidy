pub mod expansion;
pub mod params;
pub mod ref_rewriting;

use params::ParamDef;

/// A parsed custom resource template (imported document with $params).
/// Stores the raw YAML string for re-parsing during expansion.
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    pub params: Vec<ParamDef>,
    pub raw_body: String,
    pub location: String,
}
