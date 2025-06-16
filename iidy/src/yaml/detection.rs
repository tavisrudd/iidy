//! YAML specification and document type detection
//!
//! This module provides functionality to detect which YAML specification version
//! should be used for parsing a document, based on content analysis and heuristics.
//! It also includes document type predicates for specific formats like CloudFormation
//! and Kubernetes manifests.

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
            YamlSpecDetection::CloudFormation => true, // CloudFormation uses YAML 1.1
            YamlSpecDetection::Kubernetes => false,    // Kubernetes uses YAML 1.2
            YamlSpecDetection::Unknown => false,       // Default to YAML 1.2 strict mode
        }
    }
}

/// Check if the document appears to be a CloudFormation template
pub fn is_cloudformation_template(input: &str) -> bool {
    // CloudFormation-specific top-level keys
    let cfn_indicators = [
        "AWSTemplateFormatVersion",
        "Transform:",
        "Resources:",
        "Parameters:",
        "Outputs:",
        "Conditions:",
        "Mappings:",
        "Metadata:",
    ];

    // Look for CloudFormation indicators in the first 50 lines
    let lines: Vec<&str> = input.lines().take(50).collect();
    let content = lines.join("\n");

    // Count how many CloudFormation indicators we find
    let cfn_count = cfn_indicators
        .iter()
        .filter(|&indicator| content.contains(indicator))
        .count();

    // If we find 2+ CloudFormation indicators, it's likely a CFN template
    cfn_count >= 2
}

/// Check if the document appears to be a Kubernetes manifest
pub fn is_kubernetes_manifest(input: &str) -> bool {
    // Kubernetes-specific patterns (not currently used in detection logic)
    let _k8s_indicators = ["apiVersion:", "kind:", "metadata:", "spec:", "status:"];

    // Kubernetes API versions
    let k8s_api_versions = [
        "apps/v1",
        "v1",
        "extensions/v1beta1",
        "networking.k8s.io",
        "batch/v1",
        "autoscaling/v1",
        "rbac.authorization.k8s.io",
    ];

    // Look for Kubernetes indicators in the first 20 lines
    let lines: Vec<&str> = input.lines().take(20).collect();
    let content = lines.join("\n");

    // Check for apiVersion and kind (required for all K8s resources)
    let has_api_version = content.contains("apiVersion:");
    let has_kind = content.contains("kind:");

    // Check for known Kubernetes API versions
    let has_k8s_api = k8s_api_versions.iter().any(|&api| content.contains(api));

    // If we have apiVersion + kind + known K8s API, it's likely Kubernetes
    has_api_version && has_kind && has_k8s_api
}

#[cfg(test)]
mod tests {
    use super::*;

    // YAML specification detection tests
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
    fn test_cloudformation_spec_requires_multiple_indicators() {
        // Only one indicator should not trigger CloudFormation detection
        let yaml_content = r#"
Resources:
  something: here
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::Unknown);
    }

    #[test]
    fn test_kubernetes_spec_requires_all_indicators() {
        // Missing apiVersion should not trigger Kubernetes detection
        let yaml_content = r#"
kind: Deployment
metadata:
  name: test
"#;
        let result = detect_yaml_spec(yaml_content);
        assert_eq!(result, YamlSpecDetection::Unknown);
    }

    // CloudFormation template detection tests
    #[test]
    fn test_cloudformation_template_detection() {
        let yaml_content = r#"
AWSTemplateFormatVersion: '2010-09-09'
Description: Test template
Resources:
  MyBucket:
    Type: AWS::S3::Bucket
Parameters:
  BucketName:
    Type: String
"#;
        assert!(is_cloudformation_template(yaml_content));
    }

    #[test]
    fn test_cloudformation_requires_multiple_indicators() {
        // Only one indicator should not trigger CloudFormation detection
        let yaml_content = r#"
Resources:
  something: here
random_key: value
"#;
        assert!(!is_cloudformation_template(yaml_content));
    }

    #[test]
    fn test_cloudformation_with_transform() {
        let yaml_content = r#"
Transform: AWS::Serverless-2016-10-31
Resources:
  MyFunction:
    Type: AWS::Serverless::Function
"#;
        assert!(is_cloudformation_template(yaml_content));
    }

    // Kubernetes manifest detection tests
    #[test]
    fn test_kubernetes_manifest_detection() {
        let yaml_content = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx-deployment
spec:
  replicas: 3
"#;
        assert!(is_kubernetes_manifest(yaml_content));
    }

    #[test]
    fn test_kubernetes_template_requires_all_indicators() {
        // Missing apiVersion should not trigger Kubernetes detection
        let yaml_content = r#"
kind: Deployment
metadata:
  name: test
spec:
  replicas: 1
"#;
        assert!(!is_kubernetes_manifest(yaml_content));
    }

    #[test]
    fn test_kubernetes_with_core_v1() {
        let yaml_content = r#"
apiVersion: v1
kind: Service
metadata:
  name: my-service
spec:
  selector:
    app: MyApp
"#;
        assert!(is_kubernetes_manifest(yaml_content));
    }

    #[test]
    fn test_kubernetes_with_networking_api() {
        let yaml_content = r#"
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: test-network-policy
spec:
  podSelector: {}
"#;
        assert!(is_kubernetes_manifest(yaml_content));
    }

    #[test]
    fn test_generic_yaml_not_detected() {
        let yaml_content = r#"
some_config:
  value: test
  number: 42
nested:
  data: here
"#;
        assert!(!is_cloudformation_template(yaml_content));
        assert!(!is_kubernetes_manifest(yaml_content));
    }
}
