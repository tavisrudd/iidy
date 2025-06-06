//! Performance benchmarks for YAML preprocessing system
//!
//! Measures the performance of various preprocessing operations to establish
//! baselines and identify optimization opportunities.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use iidy::yaml::{preprocess_yaml_with_base_location, preprocess_yaml_sync};
use iidy::yaml::tags::{TagResolver, StandardTagResolver, DebugTagResolver, TracingTagResolver, TagContext};
use iidy::yaml::ast::IncludeTag;
use iidy::yaml::handlebars::engine::{create_handlebars_registry, interpolate_handlebars_string};
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

/// Benchmark tag resolution with different resolvers
fn bench_tag_resolvers(c: &mut Criterion) {
    let mut group = c.benchmark_group("tag_resolvers");
    
    let context = TagContext::new()
        .with_variable("config", Value::String("test-value".to_string()))
        .with_variable("environment", Value::String("prod".to_string()));
    
    let include_tag = IncludeTag {
        path: "config".to_string(),
        query: None,
    };
    
    // Standard resolver
    group.bench_function("standard_resolver", |b| {
        let resolver = StandardTagResolver;
        b.iter(|| {
            resolver.resolve_include(black_box(&include_tag), black_box(&context)).unwrap()
        })
    });
    
    // Debug resolver (with logging overhead)
    group.bench_function("debug_resolver", |b| {
        let resolver = DebugTagResolver::new();
        b.iter(|| {
            resolver.resolve_include(black_box(&include_tag), black_box(&context)).unwrap()
        })
    });
    
    // Tracing resolver (with timing overhead)
    group.bench_function("tracing_resolver", |b| {
        let resolver = TracingTagResolver::new();
        b.iter(|| {
            resolver.resolve_include(black_box(&include_tag), black_box(&context)).unwrap()
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
  condition: !$eq ["prod", "{{{{environment}}}}"]
  then: "{{{{config.database_host}}}}:{{{{config.database_port}}}}"
  else: "localhost:5432"

services: !$map
  source: ["api", "web", "worker"]
  transform: "{{{{app_name}}}}-{{{{item}}}}-{{{{environment}}}}"
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
  S3Buckets: !$fromPairs !$map
    source: !$ regions
    transform:
      - "{{toLowerCase app_name}}-{{item}}-{{environment}}-bucket"
      - Type: "AWS::S3::Bucket"
        Properties:
          BucketName: "{{toLowerCase app_name}}-{{item}}-{{environment}}-bucket"
          Tags: !$map
            source:
              - {Key: "Name", Value: "{{app_name}}-{{item}}-bucket"}
              - {Key: "Environment", Value: "{{environment}}"}
              - {Key: "Region", Value: "{{item}}"}
            transform: !$ item

  # Generate Auto Scaling Groups for different instance types
  AutoScalingGroups: !$mergeMap
    source: !$ instance_types
    transform: !$let
      bindings:
        instance_type: "{{item}}"
        group_name: "{{app_name}}-{{item}}-{{environment}}-asg"
      expression: !$fromPairs
        - - "{{group_name}}"
          - Type: "AWS::AutoScaling::AutoScalingGroup"
            Properties:
              LaunchTemplate:
                LaunchTemplateId: !Ref LaunchTemplate
                Version: !GetAtt LaunchTemplate.LatestVersionNumber
              MinSize: !$if
                condition: !$eq ["{{instance_type}}", "t3.micro"]
                then: 1
                else: 2
              MaxSize: !$if
                condition: !$eq ["{{instance_type}}", "t3.micro"]
                then: 3
                else: 6
              DesiredCapacity: !$if
                condition: !$eq ["{{instance_type}}", "t3.micro"]
                then: 1
                else: 2

  # Generate security groups with complex rules
  SecurityGroupRules: !$concatMap
    source: !$ regions
    transform: !$map
      source: ["web", "api", "database"]
      transform:
        GroupName: "{{app_name}}-{{item}}-{{outer}}-sg"
        IpPermissions: !$if
          condition: !$eq ["{{item}}", "database"]
          then:
            - IpProtocol: "tcp"
              FromPort: 5432
              ToPort: 5432
              SourceSecurityGroupName: "{{app_name}}-api-{{outer}}-sg"
          else: !$if
            condition: !$eq ["{{item}}", "api"]
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
  BucketNames: !$toJsonString !$mapValues
    source: !$ S3Buckets
    transform: !$ value.Properties.BucketName
    
  SecurityGroups: !$toYamlString !$ SecurityGroupRules
"#;
            
            preprocess_yaml_with_base_location(
                black_box(large_yaml),
                "large.yaml"
            ).await.unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark sync vs async preprocessing
fn bench_sync_vs_async(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("sync_vs_async");
    
    let yaml_input = r#"
$defs:
  app_name: "benchmark-app"
  environment: "test"

name: "{{app_name}}-{{environment}}"
services: !$map
  source: ["api", "web", "worker"]
  transform: "{{app_name}}-{{item}}-{{environment}}"

config: !$merge
  - name: "{{app_name}}"
    env: "{{environment}}"
  - replicas: 3
    version: "1.0.0"
"#;
    
    group.bench_function("sync_preprocessing", |b| {
        b.iter(|| {
            preprocess_yaml_sync(
                black_box(yaml_input),
                black_box("sync.yaml")
            ).unwrap()
        })
    });
    
    group.bench_function("async_preprocessing", |b| {
        b.to_async(&rt).iter(|| async {
            preprocess_yaml_with_base_location(
                black_box(yaml_input),
                "async.yaml"
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
  condition: !$eq ["prod", "{{environment}}"]
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
  source: !$ services
  transform: "{{app_name}}-{{item}}-service"
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
  source: !$ environments
  var_name: "env"
  transform: !$map
    source: !$ services
    var_name: "service"
    transform:
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
  source: ["#);
        
        for i in 0..*size {
            yaml_content.push_str(&format!("service-{}", i));
            if i < size - 1 {
                yaml_content.push_str("\", \"");
            }
        }
        
        yaml_content.push_str(r#""]
  transform: !$merge
    - name: "{{app_name}}-{{item}}"
    - !$ base_config

large_mapping: !$fromPairs !$map
  source: ["#);
        
        for i in 0..*size {
            yaml_content.push_str(&format!("key-{}", i));
            if i < size - 1 {
                yaml_content.push_str("\", \"");
            }
        }
        
        yaml_content.push_str(r#""]
  transform:
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
    bench_tag_resolvers,
    bench_preprocessing_pipeline,
    bench_sync_vs_async,
    bench_tag_types,
    bench_memory_usage
);

criterion_main!(benches);