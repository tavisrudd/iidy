//! Document type predicates for YAML content analysis
//! 
//! This module provides functions to detect specific document types based on
//! content heuristics and structural patterns.

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
    let cfn_count = cfn_indicators.iter()
        .filter(|&indicator| content.contains(indicator))
        .count();
    
    // If we find 2+ CloudFormation indicators, it's likely a CFN template
    cfn_count >= 2
}

/// Check if the document appears to be a Kubernetes manifest
pub fn is_kubernetes_manifest(input: &str) -> bool {
    // Kubernetes-specific patterns (not currently used in detection logic)
    let _k8s_indicators = [
        "apiVersion:",
        "kind:",
        "metadata:",
        "spec:",
        "status:",
    ];
    
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
    let has_k8s_api = k8s_api_versions.iter()
        .any(|&api| content.contains(api));
    
    // If we have apiVersion + kind + known K8s API, it's likely Kubernetes
    has_api_version && has_kind && has_k8s_api
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_kubernetes_requires_all_indicators() {
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