
#[cfg(feature = "enhanced-errors")]
use crate::yaml::enhanced_errors::{EnhancedPreprocessingError, SourceLocation};

/// Wrapper for variable not found errors that switches between basic and enhanced error reporting
#[allow(unused_variables)]
pub fn variable_not_found_error(
    variable: &str,
    file_path: &str,
    yaml_path: &str,
    available_vars: Vec<String>,
) -> anyhow::Error {
    #[cfg(not(feature = "enhanced-errors"))]
    {
        // Existing error format
        anyhow::anyhow!(
            "Variable '{}' not found in environment in file '{}' at path '{}'\n\
            Only variables from $defs, $imports, and local scoped variables (like 'item' in !$map) are available.",
            variable, file_path, yaml_path
        )
    }
    
    #[cfg(feature = "enhanced-errors")]
    {
        // Enhanced error format with error IDs and suggestions
        let location = SourceLocation::new(file_path, 0, 0, yaml_path);
        let error = EnhancedPreprocessingError::variable_not_found(variable, location, available_vars);
        anyhow::anyhow!("{}", error)
    }
}

/// Wrapper for type mismatch errors
#[allow(unused_variables)]
pub fn type_mismatch_error(
    expected: &str,
    found: &str,
    file_path: &str,
    yaml_path: &str,
    context: &str,
) -> anyhow::Error {
    #[cfg(not(feature = "enhanced-errors"))]
    {
        // Existing error format
        anyhow::anyhow!("{}", context)
    }
    
    #[cfg(feature = "enhanced-errors")]
    {
        // Enhanced error format
        let location = SourceLocation::new(file_path, 0, 0, yaml_path);
        let error = EnhancedPreprocessingError::type_mismatch(expected, found, location, context);
        anyhow::anyhow!("{}", error)
    }
}

/// Wrapper for missing required field errors
#[allow(unused_variables)]
pub fn missing_required_field_error(
    tag_name: &str,
    missing_field: &str,
    file_path: &str,
    yaml_path: &str,
    required_fields: Vec<String>,
) -> anyhow::Error {
    #[cfg(not(feature = "enhanced-errors"))]
    {
        // Existing error format
        anyhow::anyhow!(
            "Tag {} is missing required field: {}",
            tag_name, missing_field
        )
    }
    
    #[cfg(feature = "enhanced-errors")]
    {
        // Enhanced error format
        let location = SourceLocation::new(file_path, 0, 0, yaml_path);
        let error = EnhancedPreprocessingError::missing_required_field(
            tag_name,
            missing_field,
            location,
            required_fields,
        );
        anyhow::anyhow!("{}", error)
    }
}