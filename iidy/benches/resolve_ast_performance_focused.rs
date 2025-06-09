//! Focused performance benchmark for specific regression cases
//!
//! This benchmark targets the specific performance regressions we identified
//! and tests different optimization approaches.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iidy::yaml::preprocessor::YamlPreprocessor;
use iidy::yaml::tags::{TagContext, TagResolver, StandardTagResolver};
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::ast::*;
use serde_yaml::Value;

/// Test direct TagResolver usage vs preprocessor delegation
fn bench_direct_vs_delegated(c: &mut Criterion) {
    let mut group = c.benchmark_group("direct_vs_delegated");
    
    // Setup
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, true);
    let resolver = StandardTagResolver;
    
    let context = TagContext::new()
        .with_variable("service", Value::String("api-server".to_string()));
    
    // Plain string test case (biggest regression)
    let plain_string = YamlAst::String("plain-text-no-interpolation".to_string());
    
    group.bench_function("plain_string_via_preprocessor", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(plain_string.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("plain_string_direct_resolver", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&plain_string), black_box(&context)).unwrap()
        })
    });
    
    // Large mapping test case
    let mut large_pairs = Vec::new();
    for i in 0..20 {
        large_pairs.push((
            YamlAst::String(format!("key_{}", i)),
            YamlAst::String(format!("{{service}}_value_{}", i))
        ));
    }
    let large_mapping = YamlAst::Mapping(large_pairs);
    
    group.bench_function("large_mapping_via_preprocessor", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(large_mapping.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("large_mapping_direct_resolver", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&large_mapping), black_box(&context)).unwrap()
        })
    });
    
    // Large sequence test case
    let large_sequence = YamlAst::Sequence(
        (0..50).map(|i| YamlAst::String(format!("{{service}}-item-{}", i))).collect()
    );
    
    group.bench_function("large_sequence_via_preprocessor", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(large_sequence.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("large_sequence_direct_resolver", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&large_sequence), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Test string processing optimizations
fn bench_string_processing_optimizations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_processing");
    
    let resolver = StandardTagResolver;
    let context = TagContext::new()
        .with_variable("service", Value::String("api-server".to_string()));
    
    // Test different string patterns
    let plain_string = YamlAst::String("plain-text-no-interpolation".to_string());
    let simple_handlebars = YamlAst::String("{{service}}".to_string());
    let complex_handlebars = YamlAst::String("{{service}}-production-v1".to_string());
    
    group.bench_function("plain_string", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&plain_string), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("simple_handlebars", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&simple_handlebars), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("complex_handlebars", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&complex_handlebars), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Test memory allocation patterns
fn bench_allocation_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("allocation_patterns");
    
    let resolver = StandardTagResolver;
    let context = TagContext::new()
        .with_variable("service", Value::String("api-server".to_string()));
    
    // Test vector pre-allocation for sequences
    let small_sequence = YamlAst::Sequence(vec![
        YamlAst::String("item1".to_string()),
        YamlAst::String("item2".to_string()),
        YamlAst::String("item3".to_string()),
    ]);
    
    let medium_sequence = YamlAst::Sequence(
        (0..10).map(|i| YamlAst::String(format!("item_{}", i))).collect()
    );
    
    let large_sequence = YamlAst::Sequence(
        (0..50).map(|i| YamlAst::String(format!("item_{}", i))).collect()
    );
    
    group.bench_function("small_sequence", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&small_sequence), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("medium_sequence", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&medium_sequence), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("large_sequence", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&large_sequence), black_box(&context)).unwrap()
        })
    });
    
    // Test mapping pre-allocation
    let small_mapping = YamlAst::Mapping(vec![
        (YamlAst::String("key1".to_string()), YamlAst::String("value1".to_string())),
        (YamlAst::String("key2".to_string()), YamlAst::String("value2".to_string())),
    ]);
    
    let large_mapping = YamlAst::Mapping(
        (0..20).map(|i| (
            YamlAst::String(format!("key_{}", i)),
            YamlAst::String(format!("value_{}", i))
        )).collect()
    );
    
    group.bench_function("small_mapping", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&small_mapping), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("large_mapping", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&large_mapping), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_direct_vs_delegated,
    bench_string_processing_optimizations,
    bench_allocation_patterns
);

criterion_main!(benches);