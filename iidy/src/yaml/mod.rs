//! YAML preprocessing module for iidy
//! 
//! This module implements the custom YAML preprocessing language that allows
//! advanced template composition, data imports, and transformations.
//! 
//! ## Two-Phase Processing Architecture
//! 
//! The preprocessing follows the same two-phase approach as the original iidy-js:
//! 
//! **Phase 1 - Import Loading and Environment Building:**
//! - Parse YAML with custom schema recognition
//! - Copy `$defs` values to environment (unprocessed)
//! - Load all `$imports` with handlebars interpolation in paths
//! - Build complete environment with all imports and definitions
//! 
//! **Phase 2 - Tag Processing and Final Resolution:**
//! - Process all custom tags using visitor pattern
//! - Apply handlebars interpolation to final values
//! - Resolve `!$` includes with dot notation
//! - Generate final processed output

pub mod ast;
pub mod parser;
pub mod tags;
pub mod imports;
pub mod handlebars;

pub use ast::*;
pub use parser::parse_yaml_with_custom_tags;
pub use tags::TagContext;

use anyhow::Result;
use serde_yaml::Value;
use std::path::PathBuf;

use crate::yaml::imports::{ImportLoader, ImportRecord, EnvValues};
use crate::yaml::imports::loaders::ProductionImportLoader;

/// Main entry point for YAML preprocessing
/// 
/// Takes raw YAML text and processes all custom tags and preprocessing directives
/// to produce a final YAML document ready for standard deserialization.
pub async fn preprocess_yaml(input: &str) -> Result<Value> {
    preprocess_yaml_with_base_location(input, "input.yaml").await
}

/// Preprocess YAML with a specific base location for resolving relative imports
pub async fn preprocess_yaml_with_base_location(input: &str, base_location: &str) -> Result<Value> {
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    preprocessor.process(input, base_location).await
}

/// YAML preprocessor that handles the two-phase processing pipeline
pub struct YamlPreprocessor<L: ImportLoader> {
    import_loader: L,
}

impl<L: ImportLoader> YamlPreprocessor<L> {
    pub fn new(import_loader: L) -> Self {
        Self { import_loader }
    }

    /// Main processing entry point - implements the two-phase pipeline
    pub async fn process(&mut self, input: &str, base_location: &str) -> Result<Value> {
        // Parse YAML with custom tag support
        let ast = parser::parse_yaml_with_custom_tags(input)?;
        
        // Phase 1: Import loading and environment building
        let mut env_values = EnvValues::new();
        let mut import_records = Vec::new();
        self.load_imports_and_defs(&ast, base_location, &mut env_values, &mut import_records).await?;
        
        // Phase 2: Tag processing and final resolution
        let mut context = TagContext::new()
            .with_base_path(PathBuf::from(base_location));
        
        // Add all environment variables to context
        for (key, value) in env_values {
            context = context.with_variable(&key, value);
        }
            
        self.resolve_ast_with_context(ast, &context)
    }

    /// Phase 1: Load all imports and definitions to build the complete environment
    async fn load_imports_and_defs(
        &self,
        ast: &YamlAst,
        base_location: &str,
        env_values: &mut EnvValues,
        import_records: &mut Vec<ImportRecord>,
    ) -> Result<()> {
        // Look for $imports and $defs in the root mapping
        if let YamlAst::Mapping(pairs) = ast {
            for (key, value) in pairs {
                if let YamlAst::String(key_str) = key {
                    match key_str.as_str() {
                        "$defs" => {
                            self.process_defs(value, env_values)?;
                        }
                        "$imports" => {
                            self.process_imports(value, base_location, env_values, import_records).await?;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    /// Process $defs by copying values to environment (unprocessed)
    fn process_defs(&self, defs_ast: &YamlAst, env_values: &mut EnvValues) -> Result<()> {
        if let YamlAst::Mapping(pairs) = defs_ast {
            for (key, value) in pairs {
                if let YamlAst::String(key_str) = key {
                    // Check for collisions with existing imports
                    if env_values.contains_key(key_str) {
                        return Err(anyhow::anyhow!(
                            "\"{}\" in $defs collides with the same name in $imports",
                            key_str
                        ));
                    }
                    
                    // Convert AST to Value for storage (will be processed later in Phase 2)
                    let value_raw = self.ast_to_value_unprocessed(value.clone())?;
                    env_values.insert(key_str.clone(), value_raw);
                }
            }
        }
        Ok(())
    }

    /// Process $imports by loading external data with handlebars interpolation
    async fn process_imports(
        &self,
        imports_ast: &YamlAst,
        base_location: &str,
        env_values: &mut EnvValues,
        import_records: &mut Vec<ImportRecord>,
    ) -> Result<()> {
        if let YamlAst::Mapping(pairs) = imports_ast {
            for (key, value) in pairs {
                if let (YamlAst::String(import_key), YamlAst::String(location)) = (key, value) {
                    // Check for collisions
                    if env_values.contains_key(import_key) {
                        return Err(anyhow::anyhow!(
                            "\"{}\" in $imports collides with the same name in $defs",
                            import_key
                        ));
                    }

                    // Apply handlebars interpolation to import location using current env_values
                    let resolved_location = self.interpolate_import_location(location, env_values)?;
                    
                    // Load the import
                    let import_data = self.import_loader.load(&resolved_location, base_location).await?;
                    
                    // Add to environment
                    env_values.insert(import_key.clone(), import_data.doc);
                    
                    // Record for metadata
                    import_records.push(ImportRecord {
                        key: Some(import_key.clone()),
                        from: base_location.to_string(),
                        imported: import_data.resolved_location,
                        sha256_digest: self.compute_sha256(&import_data.data),
                    });
                    
                    // TODO: Recursively process nested imports in imported documents
                }
            }
        }
        Ok(())
    }

    /// Apply handlebars interpolation to import location
    fn interpolate_import_location(&self, location: &str, env_values: &EnvValues) -> Result<String> {
        use crate::yaml::handlebars::interpolate_handlebars_string;
        
        // Check if location contains handlebars syntax
        if location.contains("{{") && location.contains("}}") {
            // Convert env_values from serde_yaml::Value to serde_json::Value for handlebars
            let mut json_env = std::collections::HashMap::new();
            for (key, yaml_value) in env_values {
                let json_value = yaml_value_to_json_value(yaml_value)?;
                json_env.insert(key.clone(), json_value);
            }
            
            interpolate_handlebars_string(location, &json_env, "import-location")
                .map_err(|e| anyhow::anyhow!("Failed to interpolate import location '{}': {}", location, e))
        } else {
            Ok(location.to_string())
        }
    }

    /// Compute SHA256 hash for import tracking
    fn compute_sha256(&self, data: &str) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Convert AST to Value without processing (for Phase 1 storage)
    fn ast_to_value_unprocessed(&self, ast: YamlAst) -> Result<Value> {
        match ast {
            YamlAst::Null => Ok(Value::Null),
            YamlAst::Bool(b) => Ok(Value::Bool(b)),
            YamlAst::Number(n) => {
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    Ok(Value::Number(serde_yaml::Number::from(n as i64)))
                } else {
                    Ok(Value::Number(serde_yaml::Number::from(n)))
                }
            }
            YamlAst::String(s) => Ok(Value::String(s)),
            YamlAst::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    result.push(self.ast_to_value_unprocessed(item)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(pairs) => {
                let mut result = serde_yaml::Mapping::new();
                for (key, value) in pairs {
                    let key_val = self.ast_to_value_unprocessed(key)?;
                    let value_val = self.ast_to_value_unprocessed(value)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(_) => {
                // Store preprocessing tags as a special marker for Phase 2
                Ok(Value::String("__PREPROCESSING_TAG__".to_string()))
            }
            YamlAst::UnknownYamlTag(tag) => {
                // Store unknown tags by converting their value
                self.ast_to_value_unprocessed(*tag.value)
            }
        }
    }

    /// Phase 2: Resolve AST with complete environment context
    pub fn resolve_ast_with_context(&mut self, ast: YamlAst, context: &TagContext) -> Result<Value> {
        match ast {
            YamlAst::Null => Ok(Value::Null),
            YamlAst::Bool(b) => Ok(Value::Bool(b)),
            YamlAst::Number(n) => Ok(Value::Number(serde_yaml::Number::from(n))),
            YamlAst::String(s) => {
                // Process handlebars templates in strings
                self.process_string_with_handlebars(s, context)
            },
            YamlAst::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    result.push(self.resolve_ast_with_context(item, context)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                for (key, value) in map {
                    let key_val = self.resolve_ast_with_context(key, context)?;
                    let value_val = self.resolve_ast_with_context(value, context)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(tag) => {
                self.resolve_preprocessing_tag_with_context(tag, context)
            },
            YamlAst::UnknownYamlTag(_) => todo!()
        }
    }

    fn resolve_preprocessing_tag(&mut self, tag: PreprocessingTag) -> Result<Value> {
        self.resolve_preprocessing_tag_with_context(tag, &TagContext::new())
    }

    fn process_string_with_handlebars(&self, s: String, context: &TagContext) -> Result<Value> {
        use crate::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;
        
        // Check if string contains handlebars syntax
        if !s.contains("{{") {
            return Ok(Value::String(s));
        }
        
        // Convert TagContext variables from serde_yaml::Value to serde_json::Value
        let mut env_values: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, yaml_value) in &context.variables {
            let json_value = yaml_value_to_json_value(yaml_value)?;
            env_values.insert(key.clone(), json_value);
        }
        
        // Apply handlebars interpolation to the string
        match interpolate_handlebars_string(&s, &env_values, "yaml-string") {
            Ok(processed_string) => Ok(Value::String(processed_string)),
            Err(e) => Err(anyhow::anyhow!("Handlebars processing failed: {}", e)),
        }
    }

    fn resolve_preprocessing_tag_with_context(&mut self, tag: PreprocessingTag, context: &TagContext) -> Result<Value> {
        use crate::yaml::tags::*;
        
        match tag {
            PreprocessingTag::Include(include_tag) => {
                resolve_include_tag(&include_tag, context)
            }
            PreprocessingTag::If(if_tag) => {
                resolve_if_tag(&if_tag, context, self)
            }
            PreprocessingTag::Map(map_tag) => {
                resolve_map_tag(&map_tag, context, self)
            }
            PreprocessingTag::Merge(merge_tag) => {
                resolve_merge_tag(&merge_tag, context, self)
            }
            PreprocessingTag::Concat(concat_tag) => {
                resolve_concat_tag(&concat_tag, context, self)
            }
            PreprocessingTag::Let(let_tag) => {
                resolve_let_tag(&let_tag, context, self)
            }
            PreprocessingTag::Eq(eq_tag) => {
                resolve_eq_tag(&eq_tag, context, self)
            }
            PreprocessingTag::Not(not_tag) => {
                resolve_not_tag(&not_tag, context, self)
            }
            PreprocessingTag::Split(split_tag) => {
                resolve_split_tag(&split_tag, context, self)
            }
            PreprocessingTag::Join(join_tag) => {
                resolve_join_tag(&join_tag, context, self)
            }
        }
    }
}

impl<L: ImportLoader> Default for YamlPreprocessor<L> 
where 
    L: Default 
{
    fn default() -> Self {
        Self::new(L::default())
    }
}

impl<L: ImportLoader> tags::AstResolver for YamlPreprocessor<L> {
    fn resolve_ast(&self, ast: &YamlAst, context: &tags::TagContext) -> Result<Value> {
        // Create a temporary preprocessor for synchronous resolution
        // TODO: Refactor AstResolver trait to support async properly
        let loader = ProductionImportLoader::new();
        let mut temp_preprocessor = YamlPreprocessor::new(loader);
        temp_preprocessor.resolve_ast_with_context(ast.clone(), context)
    }
}

/// Convert serde_yaml::Value to serde_json::Value for handlebars processing
fn yaml_value_to_json_value(yaml_value: &Value) -> Result<serde_json::Value> {
    match yaml_value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(u)))
            } else if let Some(f) = n.as_f64() {
                Ok(serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Sequence(seq) => {
            let mut json_seq = Vec::new();
            for item in seq {
                json_seq.push(yaml_value_to_json_value(item)?);
            }
            Ok(serde_json::Value::Array(json_seq))
        }
        Value::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                let key_str = match k {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.as_f64().unwrap_or(0.0).to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", k), // fallback for other types
                };
                json_map.insert(key_str, yaml_value_to_json_value(v)?);
            }
            Ok(serde_json::Value::Object(json_map))
        }
        Value::Tagged(_) => Err(anyhow::anyhow!("Tagged values not supported in handlebars conversion")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::imports::loaders::ProductionImportLoader;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_two_phase_processing_with_defs() -> Result<()> {
        let yaml_input = r#"
$defs:
  environment: "test"
  app_name: "my-app"

stack_name: "{{app_name}}-{{environment}}"
region: "us-west-2"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // Check that the environment variables were properly resolved
        if let Value::Mapping(map) = result {
            if let Some(Value::String(stack_name)) = map.get(&Value::String("stack_name".to_string())) {
                assert_eq!(stack_name, "my-app-test");
            } else {
                panic!("Expected stack_name to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_two_phase_processing_with_file_import() -> Result<()> {
        // Create a temporary file with some data
        let mut temp_file = NamedTempFile::with_suffix(".yaml")?;
        writeln!(temp_file, "database_host: db.example.com")?;
        writeln!(temp_file, "database_port: 5432")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let yaml_input = format!(r#"
$imports:
  config: "{}"

database:
  host: !$ config.database_host
  port: !$ config.database_port
"#, temp_path);

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(&yaml_input, "test.yaml").await?;

        // The database values should be resolved from the imported file
        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(database)) = map.get(&Value::String("database".to_string())) {
                if let Some(Value::String(host)) = database.get(&Value::String("host".to_string())) {
                    assert_eq!(host, "db.example.com");
                }
                if let Some(Value::String(port)) = database.get(&Value::String("port".to_string())) {
                    assert_eq!(port, "5432");
                }
            } else {
                panic!("Expected database mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_two_phase_processing_with_conditional() -> Result<()> {
        let yaml_input = r#"
$defs:
  environment: "prod"

database_host: !$if
  condition: !$eq ["prod", "{{environment}}"]
  then: "prod-db.example.com"
  else: "dev-db.example.com"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // The conditional should resolve to the prod database
        if let Value::Mapping(map) = result {
            if let Some(Value::String(db_host)) = map.get(&Value::String("database_host".to_string())) {
                assert_eq!(db_host, "prod-db.example.com");
            } else {
                panic!("Expected database_host to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }
}
