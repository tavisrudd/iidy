use iidy::yaml::{detect_yaml_spec, YamlSpecDetection};

/// Tests for YAML specification auto-detection functionality
/// 
/// Validates detection of:
/// - Explicit %YAML directives (%YAML 1.1, %YAML 1.2)
/// - CloudFormation template patterns
/// - Kubernetes manifest patterns
/// - Unknown document types (default to YAML 1.2)

#[test]
fn test_explicit_yaml_11_directive() {
    let yaml_input = r#"
%YAML 1.1
---
test:
  value: yes
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::ExplicitV11);
    assert!(detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_explicit_yaml_12_directive() {
    let yaml_input = r#"
%YAML 1.2
---
test:
  value: yes
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::ExplicitV12);
    assert!(!detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_cloudformation_template_detection() {
    let yaml_input = r#"
AWSTemplateFormatVersion: '2010-09-09'
Description: Test CloudFormation template

Resources:
  TestBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: test-bucket

Outputs:
  BucketName:
    Value: !Ref TestBucket
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::CloudFormation);
    assert!(detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_cloudformation_minimal_detection() {
    let yaml_input = r#"
Transform: AWS::Serverless-2016-10-31

Resources:
  TestFunction:
    Type: AWS::Serverless::Function
    Properties:
      Handler: index.handler
      Runtime: nodejs18.x
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::CloudFormation);
    assert!(detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_kubernetes_deployment_detection() {
    let yaml_input = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deployment
spec:
  replicas: 3
  selector:
    matchLabels:
      app: test
  template:
    metadata:
      labels:
        app: test
    spec:
      containers:
      - name: test
        image: nginx:1.21
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::Kubernetes);
    assert!(!detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_kubernetes_service_detection() {
    let yaml_input = r#"
apiVersion: v1
kind: Service
metadata:
  name: test-service
spec:
  selector:
    app: test
  ports:
  - port: 80
    targetPort: 8080
  type: ClusterIP
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::Kubernetes);
    assert!(!detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_unknown_document_type() {
    let yaml_input = r#"
application:
  name: test-app
  config:
    database:
      host: localhost
      port: 5432
    cache:
      enabled: true
      ttl: 300
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::Unknown);
    assert!(!detection.should_use_yaml_11_compatibility()); // Default to YAML 1.2
}

#[test]
fn test_explicit_directive_takes_precedence() {
    // Even if document looks like CloudFormation, explicit directive wins
    let yaml_input = r#"
%YAML 1.2
---
AWSTemplateFormatVersion: '2010-09-09'
Resources:
  TestBucket:
    Type: AWS::S3::Bucket
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::ExplicitV12);
    assert!(!detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_yaml_directive_with_comments() {
    let yaml_input = r#"
# This is a YAML 1.1 document
%YAML 1.1
---
# Configuration
config:
  enabled: yes
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::ExplicitV11);
    assert!(detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_cloudformation_with_parameters_and_conditions() {
    let yaml_input = r#"
AWSTemplateFormatVersion: '2010-09-09'

Parameters:
  Environment:
    Type: String
    Default: dev

Conditions:
  IsProduction: !Equals [!Ref Environment, prod]

Resources:
  TestBucket:
    Type: AWS::S3::Bucket
    Condition: IsProduction

Mappings:
  RegionMap:
    us-east-1:
      AMI: ami-12345678
    us-west-2:
      AMI: ami-87654321
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::CloudFormation);
    assert!(detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_kubernetes_with_custom_resource() {
    let yaml_input = r#"
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: test-network-policy
  namespace: default
spec:
  podSelector:
    matchLabels:
      app: test
  policyTypes:
  - Ingress
  - Egress
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::Kubernetes);
    assert!(!detection.should_use_yaml_11_compatibility());
}

#[test]
fn test_false_positive_resistance() {
    // Document with some CloudFormation-like words but not actually CloudFormation
    let yaml_input = r#"
database:
  resources:
    cpu: 2
    memory: 4Gi
  parameters:
    max_connections: 100
  outputs:
    - connection_string
    - port
"#;

    let detection = detect_yaml_spec(yaml_input);
    assert_eq!(detection, YamlSpecDetection::Unknown);
    assert!(!detection.should_use_yaml_11_compatibility());
}