//! YAML specification detection
//! 
//! This module provides functionality to detect which YAML specification version
//! should be used for parsing a document, based on content analysis and heuristics.

use crate::yaml::doc_type_predicates::{is_cloudformation_template, is_kubernetes_manifest};

/// Detect YAML specification version from document content
/// 
/// Checks for:
/// 1. Explicit %YAML directives (%YAML 1.1 or %YAML 1.2)
/// 2. CloudFormation-specific top-level keys (AWSTemplateFormatVersion, Resources, etc.)
/// 3. Kubernetes-specific patterns (apiVersion, kind, etc.)
pub fn detect_yaml_spec(input: &str) -> YamlSpecDetection {
    // Check for explicit YAML directive first
    let lines = input.lines().take(5); // YAML directive should be in first few lines
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("%YAML") {
            if trimmed.contains("1.1") {
                return YamlSpecDetection::ExplicitV11;
            } else if trimmed.contains("1.2") {
                return YamlSpecDetection::ExplicitV12;
            }
        }
    }
    
    // Check for CloudFormation indicators
    if is_cloudformation_template(input) {
        return YamlSpecDetection::CloudFormation;
    }
    
    // Check for Kubernetes indicators  
    if is_kubernetes_manifest(input) {
        return YamlSpecDetection::Kubernetes;
    }
    
    // Default to YAML 1.2 if no specific indicators found
    YamlSpecDetection::Unknown
}

/// Result of YAML specification detection
#[derive(Debug, Clone, PartialEq)]
pub enum YamlSpecDetection {
    /// Explicit %YAML 1.1 directive found
    ExplicitV11,
    /// Explicit %YAML 1.2 directive found  
    ExplicitV12,
    /// CloudFormation template detected (prefer YAML 1.1)
    CloudFormation,
    /// Kubernetes manifest detected (prefer YAML 1.2)
    Kubernetes,
    /// Could not determine type (default to YAML 1.2)
    Unknown,
}

impl YamlSpecDetection {
    /// Convert detection result to boolean for YAML 1.1 compatibility mode
    pub fn should_use_yaml_11_compatibility(&self) -> bool {
        match self {
            YamlSpecDetection::ExplicitV11 => true,
            YamlSpecDetection::ExplicitV12 => false,
            YamlSpecDetection::CloudFormation => true,  // CloudFormation uses YAML 1.1
            YamlSpecDetection::Kubernetes => false,     // Kubernetes uses YAML 1.2
            YamlSpecDetection::Unknown => false,        // Default to YAML 1.2 strict mode
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_yaml_11_directive() {
        let yaml_content = "%YAML 1.1\n---\ntest: value";
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::ExplicitV11);
        assert!(result.should_use_yaml_11_compatibility());
    }

    #[test]
    fn test_explicit_yaml_12_directive() {
        let yaml_content = "%YAML 1.2\n---\ntest: value";
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::ExplicitV12);
        assert!(!result.should_use_yaml_11_compatibility());
    }

    #[test]
    fn test_cloudformation_detection() {
        let yaml_content = r#"
AWSTemplateFormatVersion: '2010-09-09'
Description: Test template
Resources:
  MyBucket:
    Type: AWS::S3::Bucket
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::CloudFormation);
        assert!(result.should_use_yaml_11_compatibility());
    }

    #[test]
    fn test_kubernetes_detection() {
        let yaml_content = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx-deployment
spec:
  replicas: 3
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::Kubernetes);
        assert!(!result.should_use_yaml_11_compatibility());
    }

    #[test]
    fn test_unknown_defaults_to_yaml_12() {
        let yaml_content = r#"
some_config:
  value: test
  number: 42
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::Unknown);
        assert!(!result.should_use_yaml_11_compatibility());
    }

    #[test]
    fn test_cloudformation_requires_multiple_indicators() {
        // Only one indicator should not trigger CloudFormation detection
        let yaml_content = r#"
Resources:
  something: here
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::Unknown);
    }

    #[test]
    fn test_kubernetes_requires_all_indicators() {
        // Missing apiVersion should not trigger Kubernetes detection
        let yaml_content = r#"
kind: Deployment
metadata:
  name: test
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::Unknown);
    }
}