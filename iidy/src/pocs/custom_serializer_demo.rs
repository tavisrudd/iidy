#!/usr/bin/env rust
//! Custom YAML Serializer POC 
//!
//! This POC demonstrates an alternative approach to handling CloudFormation intrinsic 
//! functions by using a custom serializer instead of post-processing string output.
//!
//! Key Benefits:
//! - Direct control over YAML tag output format
//! - No string manipulation post-processing needed  
//! - Type-safe serialization of CloudFormation expressions
//! - Support for both compact (!Ref value) and expanded formats

use anyhow::Result;
use serde_yaml::Value;
use std::io::Write;

/// Custom YAML serializer that handles CloudFormation intrinsic functions
pub struct CloudFormationYamlSerializer<W: Write> {
    writer: W,
    indent_level: usize,
    compact_intrinsics: bool,
}

impl<W: Write> CloudFormationYamlSerializer<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            indent_level: 0,
            compact_intrinsics: true,
        }
    }
    
    pub fn with_compact_intrinsics(mut self, compact: bool) -> Self {
        self.compact_intrinsics = compact;
        self
    }
    
    pub fn serialize(&mut self, value: &Value) -> Result<()> {
        self.serialize_value(value, true)
    }
    
    fn write_indent(&mut self) -> Result<()> {
        for _ in 0..self.indent_level {
            self.writer.write_all(b"  ")?;
        }
        Ok(())
    }
    
    fn serialize_value(&mut self, value: &Value, is_root: bool) -> Result<()> {
        match value {
            Value::Null => {
                if !is_root {
                    self.writer.write_all(b"null")?;
                }
                Ok(())
            }
            Value::Bool(b) => {
                self.writer.write_all(if *b { b"true" } else { b"false" })?;
                Ok(())
            }
            Value::Number(n) => {
                write!(self.writer, "{}", n)?;
                Ok(())
            }
            Value::String(s) => {
                // Check if string needs quoting
                if needs_quoting(s) {
                    write!(self.writer, "\"{}\"", escape_string(s))?;
                } else {
                    self.writer.write_all(s.as_bytes())?;
                }
                Ok(())
            }
            Value::Sequence(seq) => {
                self.serialize_sequence(seq, is_root)
            }
            Value::Mapping(map) => {
                self.serialize_mapping(map, is_root)
            }
            Value::Tagged(tagged) => {
                // Handle tagged values - mainly for future extensibility
                write!(self.writer, "!{} ", tagged.tag)?;
                self.serialize_value(&tagged.value, false)
            }
        }
    }
    
    fn serialize_sequence(&mut self, seq: &[Value], is_root: bool) -> Result<()> {
        if seq.is_empty() {
            self.writer.write_all(b"[]")?;
            return Ok(());
        }
        
        if !is_root {
            self.writer.write_all(b"\n")?;
        }
        
        for item in seq {
            self.write_indent()?;
            self.writer.write_all(b"- ")?;
            
            match item {
                Value::Sequence(_) | Value::Mapping(_) => {
                    self.indent_level += 1;
                    self.serialize_value(item, false)?;
                    self.indent_level -= 1;
                }
                _ => {
                    self.serialize_value(item, false)?;
                }
            }
            self.writer.write_all(b"\n")?;
        }
        
        Ok(())
    }
    
    fn serialize_mapping(&mut self, map: &serde_yaml::Mapping, is_root: bool) -> Result<()> {
        if map.is_empty() {
            self.writer.write_all(b"{}")?;
            return Ok(());
        }
        
        // Check if this is a CloudFormation intrinsic function mapping
        if let Some(cfn_result) = self.try_serialize_as_cloudformation_intrinsic(map)? {
            self.writer.write_all(cfn_result.as_bytes())?;
            return Ok(());
        }
        
        if !is_root {
            self.writer.write_all(b"\n")?;
        }
        
        for (key, value) in map {
            self.write_indent()?;
            self.serialize_value(key, false)?;
            self.writer.write_all(b": ")?;
            
            match value {
                Value::Sequence(_) | Value::Mapping(_) => {
                    self.indent_level += 1;
                    self.serialize_value(value, false)?;
                    self.indent_level -= 1;
                }
                _ => {
                    self.serialize_value(value, false)?;
                }
            }
            self.writer.write_all(b"\n")?;
        }
        
        Ok(())
    }
    
    /// Try to serialize a mapping as a CloudFormation intrinsic function
    /// Returns Some(string) if it's a CF intrinsic, None otherwise
    fn try_serialize_as_cloudformation_intrinsic(&self, map: &serde_yaml::Mapping) -> Result<Option<String>> {
        // CloudFormation intrinsic functions have exactly one key starting with '!'
        if map.len() != 1 {
            return Ok(None);
        }
        
        let (key, value) = map.iter().next().unwrap();
        
        if let Value::String(key_str) = key {
            if key_str.starts_with('!') {
                // This is a CloudFormation intrinsic function
                return Ok(Some(self.format_cloudformation_intrinsic(key_str, value)?));
            }
        }
        
        Ok(None)
    }
    
    /// Format a CloudFormation intrinsic function in proper YAML tag syntax
    fn format_cloudformation_intrinsic(&self, tag: &str, value: &Value) -> Result<String> {
        let mut result = String::new();
        
        if self.compact_intrinsics {
            // Compact format: !Ref MyResource
            result.push_str(tag);
            result.push(' ');
            result.push_str(&self.format_value_inline(value)?);
        } else {
            // Expanded format with proper indentation
            result.push_str(tag);
            result.push('\n');
            let formatted_value = self.format_value_with_indent(value, self.indent_level + 1)?;
            result.push_str(&formatted_value);
        }
        
        Ok(result)
    }
    
    /// Format a value inline (for compact CF intrinsics)
    fn format_value_inline(&self, value: &Value) -> Result<String> {
        match value {
            Value::String(s) => {
                if needs_quoting(s) {
                    Ok(format!("\"{}\"", escape_string(s)))
                } else {
                    Ok(s.clone())
                }
            }
            Value::Number(n) => Ok(n.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
            Value::Sequence(seq) => {
                if seq.is_empty() {
                    Ok("[]".to_string())
                } else {
                    let items: Result<Vec<String>, _> = seq.iter()
                        .map(|item| self.format_value_inline(item))
                        .collect();
                    Ok(format!("[{}]", items?.join(", ")))
                }
            }
            Value::Mapping(_) => {
                // For complex structures, fall back to expanded format
                self.format_value_with_indent(value, 0)
            }
            Value::Tagged(_) => {
                // Tagged values need special handling
                Ok(format!("{:?}", value)) // Fallback - should be rare
            }
        }
    }
    
    /// Format a value with proper indentation (for expanded format)
    fn format_value_with_indent(&self, value: &Value, indent: usize) -> Result<String> {
        let mut temp_writer = Vec::new();
        let mut temp_serializer = CloudFormationYamlSerializer::new(&mut temp_writer);
        temp_serializer.indent_level = indent;
        temp_serializer.compact_intrinsics = self.compact_intrinsics;
        temp_serializer.serialize_value(value, false)?;
        Ok(String::from_utf8(temp_writer)?)
    }
}

/// Check if a string needs to be quoted in YAML
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    
    // Simple heuristic - check for special characters, spaces, or YAML reserved words
    s.contains(':') || 
    s.contains('\n') || 
    s.contains('"') || 
    s.contains('\'') ||
    s.contains('[') ||
    s.contains(']') ||
    s.contains('{') ||
    s.contains('}') ||
    s.contains(' ') ||  // Any space within the string
    s.starts_with('#') ||
    matches!(s, "true" | "false" | "null" | "~" | "yes" | "no" | "on" | "off") ||
    s.parse::<f64>().is_ok()
}

/// Escape special characters in a string for YAML
fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"', "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
}

/// POC demonstration function
pub async fn run_custom_serializer_demo() -> Result<()> {
    println!("🔧 Custom YAML Serializer POC Demo");
    println!("{}", "=".repeat(50));
    
    // Create a sample YAML structure with CloudFormation intrinsics
    let mut sample_data = serde_yaml::Mapping::new();
    
    // Add some basic properties
    sample_data.insert(
        Value::String("AWSTemplateFormatVersion".to_string()),
        Value::String("2010-09-09".to_string())
    );
    
    sample_data.insert(
        Value::String("Description".to_string()),
        Value::String("Sample CloudFormation template with intrinsic functions".to_string())
    );
    
    // Create Resources section with CloudFormation intrinsics
    let mut resources = serde_yaml::Mapping::new();
    let mut my_bucket = serde_yaml::Mapping::new();
    
    my_bucket.insert(
        Value::String("Type".to_string()),
        Value::String("AWS::S3::Bucket".to_string())
    );
    
    let mut properties = serde_yaml::Mapping::new();
    
    // !Ref example (compact format works well)
    let mut bucket_name_ref = serde_yaml::Mapping::new();
    bucket_name_ref.insert(
        Value::String("!Ref".to_string()),
        Value::String("BucketNameParameter".to_string())
    );
    properties.insert(
        Value::String("BucketName".to_string()),
        Value::Mapping(bucket_name_ref)
    );
    
    // !Sub example with array format
    let mut sub_mapping = serde_yaml::Mapping::new();
    let mut sub_array = Vec::new();
    sub_array.push(Value::String("${AWS::StackName}-${Environment}-bucket".to_string()));
    
    let mut sub_vars = serde_yaml::Mapping::new();
    let mut env_ref = serde_yaml::Mapping::new();
    env_ref.insert(
        Value::String("!Ref".to_string()),
        Value::String("Environment".to_string())
    );
    sub_vars.insert(
        Value::String("Environment".to_string()),
        Value::Mapping(env_ref)
    );
    sub_array.push(Value::Mapping(sub_vars));
    
    sub_mapping.insert(
        Value::String("!Sub".to_string()),
        Value::Sequence(sub_array)
    );
    properties.insert(
        Value::String("VersioningConfiguration".to_string()),
        Value::Mapping(sub_mapping)
    );
    
    // !GetAtt example
    let mut get_att_mapping = serde_yaml::Mapping::new();
    let mut get_att_array = Vec::new();
    get_att_array.push(Value::String("SomeOtherResource".to_string()));
    get_att_array.push(Value::String("Arn".to_string()));
    get_att_mapping.insert(
        Value::String("!GetAtt".to_string()),
        Value::Sequence(get_att_array)
    );
    properties.insert(
        Value::String("DependsOn".to_string()),
        Value::Mapping(get_att_mapping)
    );
    
    my_bucket.insert(
        Value::String("Properties".to_string()),
        Value::Mapping(properties)
    );
    
    resources.insert(
        Value::String("MyBucket".to_string()),
        Value::Mapping(my_bucket)
    );
    
    sample_data.insert(
        Value::String("Resources".to_string()),
        Value::Mapping(resources)
    );
    
    let root_value = Value::Mapping(sample_data);
    
    println!("\n📊 Original serde_yaml output:");
    println!("{}", serde_yaml::to_string(&root_value)?);
    
    println!("\n✨ Custom serializer output (compact intrinsics):");
    let mut compact_output = Vec::new();
    let mut compact_serializer = CloudFormationYamlSerializer::new(&mut compact_output)
        .with_compact_intrinsics(true);
    compact_serializer.serialize(&root_value)?;
    println!("{}", String::from_utf8(compact_output)?);
    
    println!("\n🔍 Custom serializer output (expanded intrinsics):");
    let mut expanded_output = Vec::new();
    let mut expanded_serializer = CloudFormationYamlSerializer::new(&mut expanded_output)
        .with_compact_intrinsics(false);
    expanded_serializer.serialize(&root_value)?;
    println!("{}", String::from_utf8(expanded_output)?);
    
    // Test with actual preprocessing output
    println!("\n🧪 Testing with preprocessed YAML:");
    test_with_preprocessing_output().await?;
    
    Ok(())
}

/// Test the custom serializer with actual preprocessing output
async fn test_with_preprocessing_output() -> Result<()> {
    use crate::yaml::preprocess_yaml_with_spec;
    use crate::cli::YamlSpec;
    
    let sample_yaml = r#"
$defs:
  environment: production
  bucket_prefix: my-company

Resources:
  MainBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "{{bucket_prefix}}-${AWS::StackName}-{{environment}}"
      VersioningConfiguration:
        Status: !Ref VersioningEnabled
      Tags:
        - Key: Environment
          Value: "{{environment}}"
        - Key: ManagedBy
          Value: !GetAtt [ManagementStack, Outputs.ToolName]
"#;
    
    // Process with our YAML preprocessor
    let processed = preprocess_yaml_with_spec(
        sample_yaml, 
        "test.yaml", 
        &YamlSpec::V11
    ).await?;
    
    println!("📝 Preprocessed structure:");
    println!("{}", serde_yaml::to_string(&processed)?);
    
    println!("\n🎯 Custom serializer on preprocessed output:");
    let mut custom_output = Vec::new();
    let mut custom_serializer = CloudFormationYamlSerializer::new(&mut custom_output)
        .with_compact_intrinsics(true);
    custom_serializer.serialize(&processed)?;
    println!("{}", String::from_utf8(custom_output)?);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_cloudformation_intrinsic() -> Result<()> {
        let mut mapping = serde_yaml::Mapping::new();
        mapping.insert(
            Value::String("!Ref".to_string()),
            Value::String("MyParameter".to_string())
        );
        let value = Value::Mapping(mapping);
        
        let mut output = Vec::new();
        let mut serializer = CloudFormationYamlSerializer::new(&mut output);
        serializer.serialize(&value)?;
        
        let result = String::from_utf8(output)?;
        assert_eq!(result.trim(), "!Ref MyParameter");
        
        Ok(())
    }
    
    #[test]
    fn test_cloudformation_intrinsic_with_array() -> Result<()> {
        let mut mapping = serde_yaml::Mapping::new();
        let mut array = Vec::new();
        array.push(Value::String("Resource".to_string()));
        array.push(Value::String("Attribute".to_string()));
        
        mapping.insert(
            Value::String("!GetAtt".to_string()),
            Value::Sequence(array)
        );
        let value = Value::Mapping(mapping);
        
        let mut output = Vec::new();
        let mut serializer = CloudFormationYamlSerializer::new(&mut output);
        serializer.serialize(&value)?;
        
        let result = String::from_utf8(output)?;
        assert_eq!(result.trim(), "!GetAtt [Resource, Attribute]");
        
        Ok(())
    }
    
    #[test]
    fn test_normal_mapping_not_affected() -> Result<()> {
        let mut mapping = serde_yaml::Mapping::new();
        mapping.insert(
            Value::String("Type".to_string()),
            Value::String("AWS::S3::Bucket".to_string())
        );
        mapping.insert(
            Value::String("Properties".to_string()),
            Value::Mapping(serde_yaml::Mapping::new())
        );
        let value = Value::Mapping(mapping);
        
        let mut output = Vec::new();
        let mut serializer = CloudFormationYamlSerializer::new(&mut output);
        serializer.serialize(&value)?;
        
        let result = String::from_utf8(output)?;
        assert!(result.contains("Type: \"AWS::S3::Bucket\""));  // Strings get quoted
        assert!(result.contains("Properties: {}"));
        
        Ok(())
    }
    
    #[test] 
    fn test_string_quoting() {
        assert!(needs_quoting(""));
        assert!(needs_quoting("true"));
        assert!(needs_quoting("false"));
        assert!(needs_quoting("null"));
        assert!(needs_quoting("123"));
        assert!(needs_quoting("has spaces"));
        assert!(needs_quoting("has:colon"));
        assert!(needs_quoting("[array]"));
        assert!(needs_quoting("{object}"));
        
        assert!(!needs_quoting("simple"));
        assert!(needs_quoting("AWS::S3::Bucket"));  // Contains colons, should be quoted
        assert!(!needs_quoting("MyParameter"));
    }
}