//! Performance benchmarks for YAML preprocessing system
//!
//! Measures the performance of various preprocessing operations to establish
//! baselines and identify optimization opportunities.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use iidy::yaml::preprocess_yaml_with_base_location;
// Removed complex tag resolver imports that aren't available
use iidy::yaml::handlebars::interpolate_handlebars_string;
use serde_yaml::Value;
use std::collections::HashMap;
use tempfile::NamedTempFile;
use std::io::Write;
use tokio::runtime::Runtime;

/// Benchmark basic handlebars template interpolation
fn bench_handlebars_interpolation(c: &mut Criterion) {
    let mut group = c.benchmark_group("handlebars_interpolation");
    
    let mut env_values = HashMap::new();
    env_values.insert("name".to_string(), serde_json::Value::String("test-app".to_string()));
    env_values.insert("environment".to_string(), serde_json::Value::String("prod".to_string()));
    env_values.insert("version".to_string(), serde_json::Value::String("1.0.0".to_string()));
    
    // Simple template
    group.bench_function("simple_template", |b| {
        b.iter(|| {
            interpolate_handlebars_string(
                black_box("{{name}}-{{environment}}"),
                black_box(&env_values),
                "benchmark"
            ).unwrap()
        })
    });
    
    // Complex template with helpers
    group.bench_function("complex_template", |b| {
        b.iter(|| {
            interpolate_handlebars_string(
                black_box("{{toUpperCase name}}-{{toLowerCase environment}}-v{{version}}"),
                black_box(&env_values),
                "benchmark"
            ).unwrap()
        })
    });
    
    // Template with base64 encoding
    group.bench_function("encoding_template", |b| {
        b.iter(|| {
            interpolate_handlebars_string(
                black_box("{{base64 name}}.{{sha256 environment}}"),
                black_box(&env_values),
                "benchmark"
            ).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark basic YAML parsing without complex tag resolution
fn bench_yaml_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("yaml_parsing");
    
    let simple_yaml = r#"
name: "test-app"
version: "1.0.0"
config:
  debug: true
  timeout: 30
"#;
    
    let complex_yaml = r#"
name: "complex-app"
version: "2.0.0"
services:
  - name: "api"
    port: 8080
    env:
      - name: "DATABASE_URL"
        value: "postgres://localhost:5432/app"
  - name: "web"
    port: 3000
    env:
      - name: "API_URL"
        value: "http://localhost:8080"
"#;
    
    group.bench_function("simple_yaml", |b| {
        b.iter(|| {
            serde_yaml::from_str::<Value>(black_box(simple_yaml)).unwrap()
        })
    });
    
    group.bench_function("complex_yaml", |b| {
        b.iter(|| {
            serde_yaml::from_str::<Value>(black_box(complex_yaml)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark complete preprocessing pipeline with different document sizes
fn bench_preprocessing_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("preprocessing_pipeline");
    
    // Small document (basic template with a few variables)
    let small_yaml = r#"
$defs:
  app_name: "test-app"
  environment: "prod"

name: "{{app_name}}-{{environment}}"
region: "us-west-2"
"#;
    
    group.bench_function("small_document", |b| {
        b.to_async(&rt).iter(|| async {
            preprocess_yaml_with_base_location(
                black_box(small_yaml),
                "small.yaml"
            ).await.unwrap()
        })
    });
    
    // Medium document (with imports and conditionals)
    group.bench_function("medium_document", |b| {
        b.to_async(&rt).iter(|| async {
            let config_content = "database_host: db.example.com\ndatabase_port: 5432";
            let mut config_file = NamedTempFile::with_suffix(".yaml").unwrap();
            writeln!(config_file, "{}", config_content).unwrap();
            let config_path = config_file.path().to_string_lossy();
            
            let medium_yaml = format!(r#"
$defs:
  app_name: "test-app"
  environment: "prod"

$imports:
  config: "{}"

name: "{{{{app_name}}}}-{{{{environment}}}}"
database_url: !$if
  test: !$eq ["prod", "{{{{environment}}}}"]
  then: "{{{{config.database_host}}}}:{{{{config.database_port}}}}"
  else: "localhost:5432"

services: !$map
  items: ["api", "web", "worker"]
  template: "{{{{app_name}}}}-{{{{item}}}}-{{{{environment}}}}"
"#, config_path);
            
            preprocess_yaml_with_base_location(
                black_box(&medium_yaml),
                "medium.yaml"
            ).await.unwrap()
        })
    });
    
    // Large document (CloudFormation template with multiple transformations)
    group.bench_function("large_cloudformation_template", |b| {
        b.to_async(&rt).iter(|| async {
            let large_yaml = r#"
$defs:
  app_name: "large-app"
  environment: "prod"
  regions: ["us-west-2", "us-east-1", "eu-west-1"]
  instance_types: ["t3.micro", "t3.small", "t3.medium"]

AWSTemplateFormatVersion: "2010-09-09"
Description: "{{app_name}} infrastructure for {{environment}}"

Parameters:
  Environment:
    Type: String
    Default: "{{environment}}"

Resources:
  # Generate S3 buckets for each region
  S3Buckets: !$map
    items: !$ regions
    template:
      Type: "AWS::S3::Bucket"
      Properties:
        BucketName: "{{toLowerCase app_name}}-{{item}}-{{environment}}-bucket"
        Tags:
          - Key: "Name" 
            Value: "{{app_name}}-{{item}}-bucket"
          - Key: "Environment"
            Value: "{{environment}}"
          - Key: "Region"
            Value: "{{item}}"

  # Generate Auto Scaling Groups for different instance types
  AutoScalingGroups: !$map
    items: !$ instance_types
    template:
      Type: "AWS::AutoScaling::AutoScalingGroup"
      Properties:
        LaunchTemplate:
          LaunchTemplateId: !Ref LaunchTemplate
          Version: !GetAtt LaunchTemplate.LatestVersionNumber
        MinSize: !$if
          test: !$eq ["{{item}}", "t3.micro"]
          then: 1
          else: 2
        MaxSize: !$if
          test: !$eq ["{{item}}", "t3.micro"]
          then: 3
          else: 6
        DesiredCapacity: !$if
          test: !$eq ["{{item}}", "t3.micro"]
          then: 1
          else: 2

  # Generate security groups with complex rules
  SecurityGroupRules: !$concatMap
    items: !$ regions
    var: "region"
    template: !$map
      items: ["web", "api", "database"]
      template:
        GroupName: "{{app_name}}-{{item}}-{{region}}-sg"
        IpPermissions: !$if
          test: !$eq ["{{item}}", "database"]
          then:
            - IpProtocol: "tcp"
              FromPort: 5432
              ToPort: 5432
              SourceSecurityGroupName: "{{app_name}}-api-{{region}}-sg"
          else: !$if
            test: !$eq ["{{item}}", "api"]
            then:
              - IpProtocol: "tcp"
                FromPort: 8080
                ToPort: 8080
                CidrIp: "0.0.0.0/0"
            else:
              - IpProtocol: "tcp"
                FromPort: 80
                ToPort: 80
                CidrIp: "0.0.0.0/0"
              - IpProtocol: "tcp"
                FromPort: 443
                ToPort: 443
                CidrIp: "0.0.0.0/0"

Outputs:
  AppName:
    Value: "{{app_name}}"
  Environment:
    Value: "{{environment}}"
"#;
            
            preprocess_yaml_with_base_location(
                black_box(large_yaml),
                "large.yaml"
            ).await.unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark preprocessing with different document sizes
fn bench_document_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("document_sizes");
    
    // Small document
    let small_yaml = r#"
$defs:
  app_name: "benchmark-app"
  environment: "test"

name: "{{app_name}}-{{environment}}"
"#;
    
    group.bench_function("small_document", |b| {
        b.to_async(&rt).iter(|| async {
            preprocess_yaml_with_base_location(
                black_box(small_yaml),
                "small.yaml"
            ).await.unwrap()
        })
    });
    
    // Medium document
    let medium_yaml = r#"
$defs:
  app_name: "benchmark-app"
  environment: "test"

name: "{{app_name}}-{{environment}}"
services: !$map
  items: ["api", "web", "worker"]
  template: "{{app_name}}-{{item}}-{{environment}}"

config: !$merge
  - name: "{{app_name}}"
    env: "{{environment}}"
  - replicas: 3
    version: "1.0.0"
"#;
    
    group.bench_function("medium_document", |b| {
        b.to_async(&rt).iter(|| async {
            preprocess_yaml_with_base_location(
                black_box(medium_yaml),
                "medium.yaml"
            ).await.unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark different tag types to identify performance characteristics
fn bench_tag_types(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("tag_types");
    
    // Simple include tag
    group.bench_function("include_tag", |b| {
        b.to_async(&rt).iter(|| async {
            let yaml = r#"
$defs:
  message: "hello world"
  
result: !$ message
"#;
            preprocess_yaml_with_base_location(black_box(yaml), "include.yaml").await.unwrap()
        })
    });
    
    // Conditional tag
    group.bench_function("conditional_tag", |b| {
        b.to_async(&rt).iter(|| async {
            let yaml = r#"
$defs:
  environment: "prod"
  
result: !$if
  test: !$eq ["prod", "{{environment}}"]
  then: "production_config"
  else: "development_config"
"#;
            preprocess_yaml_with_base_location(black_box(yaml), "conditional.yaml").await.unwrap()
        })
    });
    
    // Map transformation
    group.bench_function("map_transformation", |b| {
        b.to_async(&rt).iter(|| async {
            let yaml = r#"
$defs:
  services: ["api", "web", "worker", "database", "cache"]
  app_name: "benchmark-app"
  
results: !$map
  items: !$ services
  template: "{{app_name}}-{{item}}-service"
"#;
            preprocess_yaml_with_base_location(black_box(yaml), "map.yaml").await.unwrap()
        })
    });
    
    // Complex nested transformation
    group.bench_function("nested_transformation", |b| {
        b.to_async(&rt).iter(|| async {
            let yaml = r#"
$defs:
  environments: ["dev", "staging", "prod"]
  services: ["api", "web", "worker"]
  app_name: "benchmark-app"
  
results: !$concatMap
  items: !$ environments
  var: "env"
  template: !$map
    items: !$ services
    var: "service"
    template:
      name: "{{app_name}}-{{service}}-{{env}}"
      environment: "{{env}}"
      type: "{{service}}"
"#;
            preprocess_yaml_with_base_location(black_box(yaml), "nested.yaml").await.unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark memory usage and allocation patterns
fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("memory_usage");
    
    // Generate progressively larger inputs to test memory scaling
    let sizes = [10, 50, 100, 250, 500];
    
    for size in sizes.iter() {
        // Generate YAML with many repeated elements
        let mut yaml_content = format!(r#"
$defs:
  app_name: "memory-test"
  base_config:
    replicas: 3
    memory: "1Gi"
    cpu: "500m"

services: !$map
  items: ["#);
        
        for i in 0..*size {
            yaml_content.push_str(&format!("service-{}", i));
            if i < size - 1 {
                yaml_content.push_str("\", \"");
            }
        }
        
        yaml_content.push_str(r#""]
  template: !$merge
    - name: "{{app_name}}-{{item}}"
    - !$ base_config

large_mapping: !$fromPairs
    - !$map
        items: ["#);
        
        for i in 0..*size {
            yaml_content.push_str(&format!("key-{}", i));
            if i < size - 1 {
                yaml_content.push_str("\", \"");
            }
        }
        
        yaml_content.push_str(r#""]
        template:
          - "{{item}}"
          - "value-{{item}}"
"#);
        
        group.bench_with_input(
            BenchmarkId::new("variable_size", size),
            size,
            |b, _size| {
                b.to_async(&rt).iter(|| async {
                    preprocess_yaml_with_base_location(
                        black_box(&yaml_content),
                        "memory.yaml"
                    ).await.unwrap()
                })
            }
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_handlebars_interpolation,
    bench_yaml_parsing,
    bench_preprocessing_pipeline,
    bench_document_sizes,
    bench_tag_types,
    bench_memory_usage
);

criterion_main!(benches);