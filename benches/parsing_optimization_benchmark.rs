//! Targeted benchmark for tree-sitter YAML parsing performance
//!
//! This benchmark tests the performance of the new tree-sitter based parser
//! with various YAML document types and sizes.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use iidy::yaml::parsing::{parse_yaml_ast_with_diagnostics, parse_yaml_from_file};
use serde_yaml::Value;
use tree_sitter::Parser;
use tree_sitter_yaml::LANGUAGE;
use url::Url;

fn test_uri() -> Url {
    Url::parse("file:///benchmark.yaml").unwrap()
}

/// Create a tree-sitter parser for YAML
fn create_yaml_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&LANGUAGE.into())
        .expect("Error loading YAML grammar");
    parser
}

/// Benchmark baseline serde_yaml parsing
fn bench_serde_yaml_baseline(c: &mut Criterion, yaml_content: &str, name: &str) {
    c.bench_function(&format!("baseline_serde_yaml_{name}"), |b| {
        b.iter(|| serde_yaml::from_str::<Value>(black_box(yaml_content)).unwrap())
    });
}

/// Benchmark baseline tree-sitter parsing (just syntax tree)
fn bench_tree_sitter_baseline(c: &mut Criterion, yaml_content: &str, name: &str) {
    let mut parser = create_yaml_parser();

    c.bench_function(&format!("baseline_tree_sitter_{name}"), |b| {
        b.iter(|| parser.parse(black_box(yaml_content), None).unwrap())
    });
}

/// Benchmark our custom parser (tree-sitter + custom tag processing)
fn bench_custom_parser(c: &mut Criterion, yaml_content: &str, name: &str) {
    c.bench_function(&format!("custom_parser_{name}"), |b| {
        b.iter(|| parse_yaml_from_file(black_box(yaml_content), black_box("test.yaml")).unwrap())
    });
}

/// Benchmark plain YAML parsing (no preprocessing tags)
fn bench_plain_yaml(c: &mut Criterion) {
    // Simple YAML without preprocessing tags
    let simple_yaml = r#"
name: "test-app"
version: "1.0.0"
config:
  debug: true
  timeout: 30
  replicas: 3
"#;

    // Baseline comparisons
    bench_serde_yaml_baseline(c, simple_yaml, "simple");
    bench_tree_sitter_baseline(c, simple_yaml, "simple");
    bench_custom_parser(c, simple_yaml, "simple");

    // YAML with basic CloudFormation tags
    let cfn_yaml = r#"
AWSTemplateFormatVersion: "2010-09-09"
Description: "Test template"

Resources:
  MyBucket:
    Type: "AWS::S3::Bucket" 
    Properties:
      BucketName: !Ref BucketNameParameter
      Tags:
        - Key: "Name"
          Value: !Sub "${AWS::StackName}-bucket"
        - Key: "Environment"
          Value: !Ref Environment

  MyRole:
    Type: "AWS::IAM::Role"
    Properties:
      RoleName: !Join 
        - "-"
        - - !Ref AWS::StackName
          - "role"
"#;

    // Baseline comparisons for CloudFormation
    bench_serde_yaml_baseline(c, cfn_yaml, "cloudformation");
    bench_tree_sitter_baseline(c, cfn_yaml, "cloudformation");
    bench_custom_parser(c, cfn_yaml, "cloudformation");
}

/// Benchmark preprocessing tag parsing performance
fn bench_preprocessing_tags(c: &mut Criterion) {
    // Simple preprocessing tags
    let simple_preprocessing = r#"
name: "test-app"
result: !$if
  test: !$eq ["prod", "dev"]
  then: "production"
  else: "development"

services: !$map
  items: ["api", "web", "worker"]
  template: "service-{{item}}"

merged: !$merge
  - name: "app"
    version: "1.0"
  - environment: "test"
    replicas: 3
"#;

    // Baseline comparisons for simple preprocessing
    bench_serde_yaml_baseline(c, simple_preprocessing, "simple_preprocessing");
    bench_tree_sitter_baseline(c, simple_preprocessing, "simple_preprocessing");
    bench_custom_parser(c, simple_preprocessing, "simple_preprocessing");

    // Complex nested preprocessing tags
    let complex_preprocessing = r#"
app_config: !$let
  app_name: "complex-app"
  environments: ["dev", "staging", "prod"]
  services: ["api", "web", "worker", "database"]
  in: !$merge
    - base:
        name: "{{app_name}}"
        version: "2.0.0"
    - environments: !$map
        items: !$ environments
        template: !$merge
          - environment: "{{item}}"
            replicas: !$if
              test: !$eq ["{{item}}", "prod"]
              then: 5
              else: 2
          - services: !$map
              items: !$ services
              template: !$merge
                - name: "{{app_name}}-{{item}}-{{environments[0]}}"
                  type: "{{item}}"
                - config: !$if
                    test: !$eq ["{{item}}", "database"]
                    then:
                      memory: "2Gi"
                      storage: "100Gi"
                    else:
                      memory: "1Gi"
                      cpu: "500m"
"#;

    // Baseline comparisons for complex preprocessing
    bench_serde_yaml_baseline(c, complex_preprocessing, "complex_preprocessing");
    bench_tree_sitter_baseline(c, complex_preprocessing, "complex_preprocessing");
    bench_custom_parser(c, complex_preprocessing, "complex_preprocessing");
}

/// Benchmark array syntax parsing (common source of complexity)
fn bench_array_syntax(c: &mut Criterion) {
    let mut group = c.benchmark_group("array_syntax");

    let array_syntax = r#"
results:
  not_value: !$not [false]
  escaped_content: !$escape [{"key": "value"}]
  parsed_yaml: !$parseYaml ["key: value"]
  json_string: !$toJsonString [{"test": "data"}]
  split_result: !$split [",", "a,b,c,d,e"]
  joined_result: !$join ["-", ["one", "two", "three", "four"]]
"#;

    group.bench_function("array_syntax_parsing", |b| {
        b.iter(|| parse_yaml_from_file(black_box(array_syntax), black_box("array.yaml")).unwrap())
    });

    group.finish();
}

/// Benchmark mapping-heavy parsing (lots of key-value extraction)
fn bench_mapping_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("mapping_heavy");

    let mapping_heavy = r#"
config_a: !$map
  items: ["item1", "item2", "item3", "item4", "item5"]
  template: "prefix-{{item}}-suffix"
  var: "item"

config_b: !$mapValues
  items:
    key1: "value1"
    key2: "value2" 
    key3: "value3"
    key4: "value4"
    key5: "value5"
  template: "processed-{{value}}"
  var: "entry"

config_c: !$groupBy
  items:
    - name: "service1"
      type: "web"
    - name: "service2" 
      type: "api"
    - name: "service3"
      type: "web"
    - name: "service4"
      type: "worker"
    - name: "service5"
      type: "api"
  key: "{{item.type}}"
  var: "item"
  template: "{{item.name}}"

config_d: !$mapListToHash
  items:
    - ["key1", "value1"]
    - ["key2", "value2"]
    - ["key3", "value3"]
    - ["key4", "value4"] 
    - ["key5", "value5"]
  template: ["{{item[0]}}", "processed-{{item[1]}}"]
  var: "item"
"#;

    group.bench_function("mapping_heavy_parsing", |b| {
        b.iter(|| {
            parse_yaml_from_file(black_box(mapping_heavy), black_box("mapping.yaml")).unwrap()
        })
    });

    group.finish();
}

/// Benchmark deeply nested structures
fn bench_deep_nesting(c: &mut Criterion) {
    let mut group = c.benchmark_group("deep_nesting");

    let deep_nesting = r#"
level1: !$let
  var1: "value1"
  in: !$let
    var2: "value2"
    in: !$let
      var3: "value3"
      in: !$let
        var4: "value4"
        in: !$merge
          - first: "{{var1}}"
            second: "{{var2}}"
          - third: "{{var3}}"
            fourth: "{{var4}}"
            nested: !$if
              test: !$eq ["{{var1}}", "value1"]
              then: !$map
                items: ["a", "b", "c"]
                template: "{{var2}}-{{item}}-{{var3}}"
              else: !$concat
                - ["{{var4}}"]
                - !$split ["-", "{{var1}}-{{var2}}"]
"#;

    group.bench_function("deep_nesting", |b| {
        b.iter(|| parse_yaml_from_file(black_box(deep_nesting), black_box("deep.yaml")).unwrap())
    });

    group.finish();
}

/// Benchmark diagnostic collection (alternative parsing API)
fn bench_diagnostic_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("diagnostic_collection");

    // Valid YAML for baseline
    let valid_yaml = r#"
Resources:
  Bucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: "test-bucket"
      Tags: !$map
        items: ["env", "app", "version"]
        template: "{{item}}: value"
"#;

    group.bench_function("valid_yaml_diagnostics", |b| {
        b.iter(|| parse_yaml_ast_with_diagnostics(black_box(valid_yaml), black_box(test_uri())))
    });

    // YAML with errors for error collection testing
    let error_yaml = r#"
test1: !$unknownTag1 value
test2: !$let
  var1: value1
  # missing 'in' field
test3: !$map
  items: [1, 2, 3]
  # missing 'template' field
"#;

    group.bench_function("error_collection_diagnostics", |b| {
        b.iter(|| parse_yaml_ast_with_diagnostics(black_box(error_yaml), black_box(test_uri())))
    });

    group.finish();
}

/// Benchmark large document parsing
fn bench_large_documents(c: &mut Criterion) {
    // Generate a large CloudFormation-style document
    let mut large_yaml = String::from("AWSTemplateFormatVersion: '2010-09-09'\nResources:\n");
    for i in 0..200 {
        large_yaml.push_str(&format!(
            r#"  Resource{i}:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "${{AWS::StackName}}-bucket-{i}"
      Tags:
        - Key: "Name"
          Value: !Ref ResourceName{i}
        - Key: "Index"
          Value: "{i}"
"#
        ));
    }

    // Baseline comparisons for large CloudFormation document
    bench_serde_yaml_baseline(c, &large_yaml, "large_cloudformation");
    bench_tree_sitter_baseline(c, &large_yaml, "large_cloudformation");
    bench_custom_parser(c, &large_yaml, "large_cloudformation");

    // Generate a large preprocessing document
    let mut large_preprocessing = String::from("configs: !$map\n  items:\n");
    for i in 0..100 {
        large_preprocessing.push_str(&format!(
            r#"    - name: "service{}"
      port: {}
      replicas: {}
"#,
            i,
            8000 + i,
            (i % 5) + 1
        ));
    }
    large_preprocessing.push_str(
        r#"  template: !$merge
    - name: "{{item.name}}"
      endpoint: "http://{{item.name}}:{{item.port}}"
    - scaling:
        replicas: "{{item.replicas}}"
        resources: !$if
          test: !$eq ["{{item.replicas}}", "5"]
          then:
            memory: "2Gi"
            cpu: "1000m"
          else:
            memory: "1Gi"
            cpu: "500m"
"#,
    );

    // Baseline comparisons for large preprocessing document
    bench_serde_yaml_baseline(c, &large_preprocessing, "large_preprocessing");
    bench_tree_sitter_baseline(c, &large_preprocessing, "large_preprocessing");
    bench_custom_parser(c, &large_preprocessing, "large_preprocessing");
}

criterion_group!(
    benches,
    bench_plain_yaml,
    bench_preprocessing_tags,
    bench_array_syntax,
    bench_mapping_heavy,
    bench_deep_nesting,
    bench_diagnostic_collection,
    bench_large_documents
);

criterion_main!(benches);
