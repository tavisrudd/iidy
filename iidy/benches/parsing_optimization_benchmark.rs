//! Targeted benchmark for serde_yaml::Value -> YamlAst parsing
//!

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iidy::yaml::parser::{ParseContext, convert_value_to_ast};
use serde_yaml::Value;

/// Benchmark convert_value_to_ast performance (our optimized function)
fn bench_plain_yaml(c: &mut Criterion) {
    let mut group = c.benchmark_group("plain_yaml");
    
    // Simple YAML without preprocessing tags
    let simple_yaml = r#"
name: "test-app"
version: "1.0.0"
config:
  debug: true
  timeout: 30
  replicas: 3
"#;
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let simple_value: Value = serde_yaml::from_str(simple_yaml).unwrap();
    let simple_context = ParseContext::new("simple.yaml", simple_yaml);
    
    group.bench_function("simple_yaml_map", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(simple_value.clone()), black_box(&simple_context)).unwrap()
        })
    });
    
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
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let cfn_value: Value = serde_yaml::from_str(cfn_yaml).unwrap();
    let cfn_context = ParseContext::new("cfn.yaml", cfn_yaml);
    
    group.bench_function("cloudformation_yaml", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(cfn_value.clone()), black_box(&cfn_context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark preprocessing tag AST conversion performance
fn bench_preprocessing_tags(c: &mut Criterion) {
    let mut group = c.benchmark_group("preprocessing_tags_ast");
    
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
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let simple_value: Value = serde_yaml::from_str(simple_preprocessing).unwrap();
    let simple_context = ParseContext::new("preprocessing.yaml", simple_preprocessing);
    
    group.bench_function("simple_preprocessing", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(simple_value.clone()), black_box(&simple_context)).unwrap()
        })
    });
    
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
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let complex_value: Value = serde_yaml::from_str(complex_preprocessing).unwrap();
    let complex_context = ParseContext::new("complex.yaml", complex_preprocessing);
    
    group.bench_function("complex_preprocessing", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(complex_value.clone()), black_box(&complex_context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark array syntax AST conversion (common source of cloning)
fn bench_array_syntax(c: &mut Criterion) {
    let mut group = c.benchmark_group("array_syntax_ast");
    
    let array_syntax = r#"
results:
  not_value: !$not [false]
  escaped_content: !$escape [{"key": "value"}]
  parsed_yaml: !$parseYaml ["key: value"]
  json_string: !$toJsonString [{"test": "data"}]
  split_result: !$split [",", "a,b,c,d,e"]
  joined_result: !$join ["-", ["one", "two", "three", "four"]]
"#;
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let array_value: Value = serde_yaml::from_str(array_syntax).unwrap();
    let array_context = ParseContext::new("array.yaml", array_syntax);
    
    group.bench_function("array_syntax_parsing", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(array_value.clone()), black_box(&array_context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark mapping-heavy AST conversion (lots of key-value extraction)
fn bench_mapping_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("mapping_heavy_ast");
    
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
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let mapping_value: Value = serde_yaml::from_str(mapping_heavy).unwrap();
    let mapping_context = ParseContext::new("mapping.yaml", mapping_heavy);
    
    group.bench_function("mapping_heavy_parsing", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(mapping_value.clone()), black_box(&mapping_context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark deeply nested structures AST conversion (context.with_path usage)
fn bench_deep_nesting(c: &mut Criterion) {
    let mut group = c.benchmark_group("deep_nesting_ast");
    
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
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let deep_value: Value = serde_yaml::from_str(deep_nesting).unwrap();
    let deep_context = ParseContext::new("deep.yaml", deep_nesting);
    
    group.bench_function("deep_nesting", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(deep_value.clone()), black_box(&deep_context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark scenarios that heavily exercise cloning AST conversion
fn bench_clone_heavy_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone_heavy_ast");
    
    let clone_heavy = r#"
# Many sequential access to same mapping keys
sequence_processing: !$map
  items: ["step1", "step2", "step3", "step4", "step5"]
  template: !$merge
    - name: "{{item}}"
      step: "{{item}}"
    - config: !$if
        test: !$eq ["{{item}}", "step1"] 
        then: !$merge
          - base: "value"
          - specific: "step1-config"
        else: !$if
          test: !$eq ["{{item}}", "step2"]
          then: !$merge
            - base: "value"
            - specific: "step2-config"
          else: !$merge
            - base: "value"
            - specific: "default-config"

# Repeated field extraction from same objects
field_extraction: !$map
  items:
    - name: "service1"
      config: {port: 8080, memory: "1Gi", replicas: 3}
    - name: "service2" 
      config: {port: 8081, memory: "2Gi", replicas: 5}
    - name: "service3"
      config: {port: 8082, memory: "1Gi", replicas: 2}
  template: !$merge
    - service_name: "{{item.name}}"
      port: "{{item.config.port}}"
    - memory: "{{item.config.memory}}"
      replicas: "{{item.config.replicas}}"
    - computed: !$join 
        - ":"
        - ["{{item.name}}", "{{item.config.port}}"]
"#;
    
    // Pre-parse YAML and create context (excluded from benchmark)
    let clone_value: Value = serde_yaml::from_str(clone_heavy).unwrap();
    let clone_context = ParseContext::new("clone.yaml", clone_heavy);
    
    group.bench_function("clone_heavy_patterns", |b| {
        b.iter(|| {
            convert_value_to_ast(black_box(clone_value.clone()), black_box(&clone_context)).unwrap()
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_plain_yaml,
    bench_preprocessing_tags,
    bench_array_syntax,
    bench_mapping_heavy,
    bench_deep_nesting,
    bench_clone_heavy_patterns
);

criterion_main!(benches);

#[cfg(test)]
mod tests {
    
    #[test]
    fn test_convert_value_to_ast_operations() {
        // Verify AST conversion operations work correctly
        let simple_yaml = r#"
name: "test"
config:
  debug: true
"#;
        let value: Value = serde_yaml::from_str(simple_yaml).unwrap();
        let context = ParseContext::new("test.yaml", simple_yaml);
        let result = convert_value_to_ast(value, &context);
        assert!(result.is_ok());
        
        let preprocessing_yaml = r#"
result: !$map
  items: ["a", "b"]
  template: "item-{{item}}"
"#;
        let value: Value = serde_yaml::from_str(preprocessing_yaml).unwrap();
        let context = ParseContext::new("test.yaml", preprocessing_yaml);
        let result = convert_value_to_ast(value, &context);
        assert!(result.is_ok());
    }
}
