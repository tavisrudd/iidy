//! Focused benchmark for resolve_ast_with_context optimization
//!
//! This benchmark specifically targets the resolve_ast_with_context function
//! which is the core orchestration function in YAML preprocessing.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iidy::yaml::{YamlPreprocessor, TagContext};
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::ast::*;
use serde_yaml::Value;

/// Benchmark core resolve_ast_with_context function performance
fn bench_resolve_ast_core_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve_ast_core_patterns");
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    
    let mut context = TagContext::new();
    context = context.with_variable("environment", Value::String("production".to_string()));
    context = context.with_variable("service_name", Value::String("api-server".to_string()));
    context = context.with_variable("region", Value::String("us-west-2".to_string()));
    
    // Small mapping (most common case)
    let small_mapping = YamlAst::Mapping(vec![
        (YamlAst::String("name".to_string()), YamlAst::String("{{service_name}}".to_string())),
        (YamlAst::String("env".to_string()), YamlAst::String("{{environment}}".to_string())),
    ]);
    
    group.bench_function("small_mapping", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(small_mapping.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Medium mapping (common in configurations)
    let medium_mapping = YamlAst::Mapping(vec![
        (YamlAst::String("name".to_string()), YamlAst::String("{{service_name}}".to_string())),
        (YamlAst::String("environment".to_string()), YamlAst::String("{{environment}}".to_string())),
        (YamlAst::String("region".to_string()), YamlAst::String("{{region}}".to_string())),
        (YamlAst::String("port".to_string()), YamlAst::Number(serde_yaml::Number::from(8080))),
        (YamlAst::String("debug".to_string()), YamlAst::Bool(false)),
        (YamlAst::String("replicas".to_string()), YamlAst::Number(serde_yaml::Number::from(3))),
    ]);
    
    group.bench_function("medium_mapping", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(medium_mapping.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Large mapping (stress test)
    let mut large_pairs = Vec::new();
    for i in 0..20 {
        large_pairs.push((
            YamlAst::String(format!("key_{}", i)),
            YamlAst::String(format!("{{service_name}}_value_{}", i))
        ));
    }
    let large_mapping = YamlAst::Mapping(large_pairs);
    
    group.bench_function("large_mapping", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(large_mapping.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Small sequence (arrays are common)
    let small_sequence = YamlAst::Sequence(vec![
        YamlAst::String("{{service_name}}-worker-1".to_string()),
        YamlAst::String("{{service_name}}-worker-2".to_string()),
        YamlAst::String("{{service_name}}-worker-3".to_string()),
    ]);
    
    group.bench_function("small_sequence", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(small_sequence.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Large sequence
    let large_sequence = YamlAst::Sequence(
        (0..50).map(|i| YamlAst::String(format!("{{service_name}}-item-{}", i))).collect()
    );
    
    group.bench_function("large_sequence", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(large_sequence.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark path tracking overhead in nested structures
fn bench_path_tracking_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("path_tracking_overhead");
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    
    let context = TagContext::new()
        .with_variable("service", Value::String("api".to_string()));
    
    // Nested structure that creates many contexts
    let nested_ast = YamlAst::Mapping(vec![
        (YamlAst::String("level1".to_string()), YamlAst::Mapping(vec![
            (YamlAst::String("level2".to_string()), YamlAst::Mapping(vec![
                (YamlAst::String("level3".to_string()), YamlAst::Mapping(vec![
                    (YamlAst::String("level4".to_string()), YamlAst::String("{{service}}".to_string())),
                ]))
            ]))
        ]))
    ]);
    
    group.bench_function("deep_nesting", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(nested_ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Wide structure (many siblings at same level)
    let mut wide_pairs = Vec::new();
    for i in 0..30 {
        wide_pairs.push((
            YamlAst::String(format!("item_{}", i)), 
            YamlAst::String("{{service}}".to_string())
        ));
    }
    let wide_ast = YamlAst::Mapping(wide_pairs);
    
    group.bench_function("wide_structure", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(wide_ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark string interpolation patterns  
fn bench_string_interpolation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_interpolation");
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    
    let context = TagContext::new()
        .with_variable("service", Value::String("api-server".to_string()))
        .with_variable("env", Value::String("production".to_string()))
        .with_variable("region", Value::String("us-west-2".to_string()));
    
    // No handlebars (should be fast path)
    let plain_string = YamlAst::String("plain-text-no-interpolation".to_string());
    
    group.bench_function("plain_string", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(plain_string.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Simple handlebars
    let simple_handlebars = YamlAst::String("{{service}}".to_string());
    
    group.bench_function("simple_handlebars", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(simple_handlebars.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Complex handlebars
    let complex_handlebars = YamlAst::String("{{service}}-{{env}}-{{region}}-suffix".to_string());
    
    group.bench_function("complex_handlebars", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(complex_handlebars.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark CloudFormation tag processing
fn bench_cloudformation_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("cloudformation_processing");
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    
    let context = TagContext::new()
        .with_variable("param", Value::String("MyParameter".to_string()));
    
    // Simple CloudFormation tags
    let ref_tag = YamlAst::CloudFormationTag(CloudFormationTag::Ref(
        Box::new(YamlAst::String("{{param}}".to_string()))
    ));
    
    group.bench_function("ref_tag", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(ref_tag.clone()), black_box(&context)).unwrap()
        })
    });
    
    let sub_tag = YamlAst::CloudFormationTag(CloudFormationTag::Sub(
        Box::new(YamlAst::String("${AWS::StackName}-{{param}}".to_string()))
    ));
    
    group.bench_function("sub_tag", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(sub_tag.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark mixed content patterns (real-world scenarios)
fn bench_mixed_content_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_content");
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    
    let context = TagContext::new()
        .with_variable("service", Value::String("api".to_string()))
        .with_variable("env", Value::String("prod".to_string()));
    
    // Realistic CloudFormation-style template
    let realistic_template = YamlAst::Mapping(vec![
        (YamlAst::String("AWSTemplateFormatVersion".to_string()), YamlAst::String("2010-09-09".to_string())),
        (YamlAst::String("Description".to_string()), YamlAst::String("{{service}} service deployment".to_string())),
        (YamlAst::String("Resources".to_string()), YamlAst::Mapping(vec![
            (YamlAst::String("MyBucket".to_string()), YamlAst::Mapping(vec![
                (YamlAst::String("Type".to_string()), YamlAst::String("AWS::S3::Bucket".to_string())),
                (YamlAst::String("Properties".to_string()), YamlAst::Mapping(vec![
                    (YamlAst::String("BucketName".to_string()), 
                     YamlAst::CloudFormationTag(CloudFormationTag::Sub(
                         Box::new(YamlAst::String("${AWS::StackName}-{{service}}-{{env}}".to_string()))
                     ))),
                ]))
            ]))
        ]))
    ]);
    
    group.bench_function("realistic_template", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(realistic_template.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_resolve_ast_core_patterns,
    bench_path_tracking_overhead,
    bench_string_interpolation_patterns,
    bench_cloudformation_processing,
    bench_mixed_content_patterns
);

criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_benchmark_setup() {
        // Verify benchmark setup works correctly
        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let context = TagContext::new()
            .with_variable("test", Value::String("value".to_string()));
        
        let ast = YamlAst::String("{{test}}".to_string());
        let result = preprocessor.resolve_ast_with_context(ast, &context);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::String("value".to_string()));
    }
}