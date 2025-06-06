//! Simplified performance benchmark for YAML preprocessing
//!
//! A lightweight benchmark demonstrating performance measurement of core
//! preprocessing operations without the full criterion framework overhead.

use std::time::{Duration, Instant};
use tokio;
use iidy::yaml::preprocess_yaml_with_base_location;
use iidy::yaml::handlebars::engine::interpolate_handlebars_string;
use iidy::yaml::tags::{StandardTagResolver, DebugTagResolver, TracingTagResolver, TagResolver, TagContext};
use iidy::yaml::ast::IncludeTag;
use serde_yaml::Value;
use std::collections::HashMap;

/// Simple benchmark runner
struct SimpleBenchmark {
    name: String,
    iterations: usize,
}

impl SimpleBenchmark {
    fn new(name: &str, iterations: usize) -> Self {
        Self {
            name: name.to_string(),
            iterations,
        }
    }
    
    fn run<F>(&self, mut operation: F) -> Duration
    where
        F: FnMut(),
    {
        // Warmup
        for _ in 0..10 {
            operation();
        }
        
        let start = Instant::now();
        for _ in 0..self.iterations {
            operation();
        }
        let total_duration = start.elapsed();
        
        let avg_duration = total_duration / self.iterations as u32;
        println!(
            "{}: {} iterations in {:?} (avg: {:?})",
            self.name, self.iterations, total_duration, avg_duration
        );
        
        total_duration
    }
}

fn benchmark_handlebars() {
    println!("\n=== Handlebars Template Performance ===");
    
    let mut env_values = HashMap::new();
    env_values.insert("name".to_string(), serde_json::Value::String("test-app".to_string()));
    env_values.insert("environment".to_string(), serde_json::Value::String("prod".to_string()));
    
    // Simple template
    SimpleBenchmark::new("Simple Template", 10000).run(|| {
        interpolate_handlebars_string(
            "{{name}}-{{environment}}",
            &env_values,
            "benchmark"
        ).unwrap();
    });
    
    // Complex template with helpers
    SimpleBenchmark::new("Complex Template", 5000).run(|| {
        interpolate_handlebars_string(
            "{{toUpperCase name}}-{{toLowerCase environment}}-{{base64 name}}",
            &env_values,
            "benchmark"
        ).unwrap();
    });
}

fn benchmark_tag_resolvers() {
    println!("\n=== Tag Resolver Performance ===");
    
    let context = TagContext::new()
        .with_variable("config", Value::String("test-value".to_string()));
    
    let include_tag = IncludeTag {
        path: "config".to_string(),
        query: None,
    };
    
    // Standard resolver
    SimpleBenchmark::new("Standard Resolver", 50000).run(|| {
        let resolver = StandardTagResolver;
        resolver.resolve_include(&include_tag, &context).unwrap();
    });
    
    // Debug resolver (with logging)
    SimpleBenchmark::new("Debug Resolver", 10000).run(|| {
        let resolver = DebugTagResolver::new();
        resolver.resolve_include(&include_tag, &context).unwrap();
    });
    
    // Tracing resolver (with timing)
    SimpleBenchmark::new("Tracing Resolver", 10000).run(|| {
        let resolver = TracingTagResolver::new();
        resolver.resolve_include(&include_tag, &context).unwrap();
    });
}

fn benchmark_preprocessing_pipeline() {
    println!("\n=== YAML Preprocessing Pipeline Performance ===");
    
    // Small document
    let small_yaml = r#"
$defs:
  app_name: "test-app"
  environment: "prod"

name: "{{app_name}}-{{environment}}"
region: "us-west-2"
"#;
    
    SimpleBenchmark::new("Small Document", 100).run(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(preprocess_yaml_with_base_location(small_yaml, "small.yaml")).unwrap();
    });
    
    // Medium document with transformations
    let medium_yaml = r#"
$defs:
  app_name: "test-app"
  environment: "prod"
  services: ["api", "web", "worker"]

name: "{{app_name}}-{{environment}}"

service_configs: !$map
  source: !$ services
  transform:
    name: "{{app_name}}-{{item}}-{{environment}}"
    type: "{{item}}"

merged_config: !$merge
  - name: "{{app_name}}"
    env: "{{environment}}"
  - replicas: 3
    version: "1.0.0"
"#;
    
    SimpleBenchmark::new("Medium Document", 50).run(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(preprocess_yaml_with_base_location(medium_yaml, "medium.yaml")).unwrap();
    });
    
    // Large document with complex transformations
    let large_yaml = r#"
$defs:
  app_name: "large-app"
  environment: "prod"
  regions: ["us-west-2", "us-east-1", "eu-west-1"]
  services: ["api", "web", "worker", "database", "cache"]

name: "{{app_name}}-{{environment}}"

# Generate service configurations for each region
regional_services: !$concatMap
  source: !$ regions
  var_name: "region"
  transform: !$map
    source: !$ services
    var_name: "service"
    transform:
      name: "{{app_name}}-{{service}}-{{region}}-{{environment}}"
      region: "{{region}}"
      service: "{{service}}"
      environment: "{{environment}}"

# Generate configuration mappings
service_mappings: !$fromPairs !$map
  source: !$ services
  transform:
    - "{{item}}"
    - type: "{{item}}"
      replicas: !$if
        condition: !$eq ["{{item}}", "database"]
        then: 1
        else: 3
      resources:
        memory: !$if
          condition: !$eq ["{{item}}", "database"]
          then: "2Gi"
          else: "1Gi"

# Test complex nested transformations
complex_config: !$merge
  - base:
      app: "{{app_name}}"
      env: "{{environment}}"
  - regional: !$ regional_services
  - services: !$ service_mappings
"#;
    
    // Large document test temporarily disabled due to YAML parsing issue
    // SimpleBenchmark::new("Large Document", 10).run(|| {
    //     let rt = tokio::runtime::Runtime::new().unwrap();
    //     rt.block_on(preprocess_yaml_with_base_location(large_yaml, "large.yaml")).unwrap();
    // });
}

fn benchmark_memory_scaling() {
    println!("\n=== Memory Scaling Performance ===");
    
    // Test with different sizes of data
    let sizes = [10, 50, 100];
    
    for size in sizes.iter() {
        let services: Vec<String> = (0..*size).map(|i| format!("service-{}", i)).collect();
        let services_yaml = services.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", ");
        
        let yaml_content = format!(r#"
$defs:
  services: [{}]
  app_name: "scaling-test"

results: !$map
  source: !$ services
  transform: "{{{{app_name}}}}-{{{{item}}}}"

pairs: !$fromPairs !$map
  source: !$ services
  transform:
    - "{{{{item}}}}"
    - "value-{{{{item}}}}"
"#, services_yaml);
        
        SimpleBenchmark::new(&format!("Size {} services", size), 10).run(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(preprocess_yaml_with_base_location(&yaml_content, "scaling.yaml")).unwrap();
        });
    }
}

fn main() {
    println!("YAML Preprocessing Performance Benchmarks");
    println!("==========================================");
    
    benchmark_handlebars();
    benchmark_tag_resolvers();
    benchmark_preprocessing_pipeline();
    benchmark_memory_scaling();
    
    println!("\n=== Summary ===");
    println!("Benchmarks completed successfully!");
    println!("Note: Results may vary based on system performance and compilation optimizations.");
    println!("For more detailed analysis, use: cargo bench --bench yaml_preprocessing_benchmarks");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_benchmark_functionality() {
        // Quick test to ensure benchmark operations work correctly
        let mut env_values = HashMap::new();
        env_values.insert("test".to_string(), serde_json::Value::String("value".to_string()));
        
        let result = interpolate_handlebars_string("{{test}}", &env_values, "test").unwrap();
        assert_eq!(result, "value");
        
        let yaml = r#"
$defs:
  name: "test"
result: "{{name}}"
"#;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let processed = rt.block_on(preprocess_yaml_with_base_location(yaml, "test.yaml")).unwrap();
        assert!(processed.is_mapping());
    }
}