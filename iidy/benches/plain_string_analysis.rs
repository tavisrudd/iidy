//! Deep dive into plain string processing performance
//!
//! This benchmark isolates different approaches to plain string processing
//! to understand the 30.9% regression.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iidy::yaml::preprocessor::YamlPreprocessor;
use iidy::yaml::tags::{TagContext, StandardTagResolver, TagResolver};
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::ast::YamlAst;
use serde_yaml::Value;

fn bench_string_processing_approaches(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_processing_approaches");
    
    // Setup test string (same as benchmark)
    let test_string = "plain-text-no-interpolation";
    let ast = YamlAst::PlainString(test_string.to_string());
    let context = TagContext::new();
    
    // Method 1: Current optimized preprocessor
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, true);
    
    group.bench_function("current_optimized_preprocessor", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Method 2: Direct resolver (bypassing preprocessor)
    let resolver = StandardTagResolver;
    
    group.bench_function("direct_resolver", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&ast), black_box(&context)).unwrap()
        })
    });
    
    // Method 3: Test individual operations
    group.bench_function("string_contains_check", |b| {
        b.iter(|| {
            black_box(!test_string.contains("{{"))
        })
    });
    
    group.bench_function("string_clone_operation", |b| {
        b.iter(|| {
            black_box(test_string.to_string())
        })
    });
    
    group.bench_function("value_string_creation", |b| {
        b.iter(|| {
            black_box(Value::String(test_string.to_string()))
        })
    });
    
    // Method 4: Test different string lengths
    let short_string = "short";
    let medium_string = "medium-length-string";
    let long_string = "this-is-a-very-long-string-with-many-characters-to-test-performance";
    
    group.bench_function("short_string_contains", |b| {
        b.iter(|| {
            black_box(!short_string.contains("{{"))
        })
    });
    
    group.bench_function("medium_string_contains", |b| {
        b.iter(|| {
            black_box(!medium_string.contains("{{"))
        })
    });
    
    group.bench_function("long_string_contains", |b| {
        b.iter(|| {
            black_box(!long_string.contains("{{"))
        })
    });
    
    // Method 5: Alternative fast path checks
    group.bench_function("memchr_contains_check", |b| {
        b.iter(|| {
            // Check for '{' character first (cheaper than substring search)
            black_box(!test_string.contains('{'))
        })
    });
    
    group.bench_function("bytes_scan_for_brace", |b| {
        b.iter(|| {
            // Manual byte scan for '{' 
            black_box(!test_string.bytes().any(|b| b == b'{'))
        })
    });
    
    // Method 6: Test what happens if we skip the check entirely
    group.bench_function("skip_contains_check_direct_clone", |b| {
        b.iter(|| {
            // What if we just always clone for strings?
            black_box(Value::String(test_string.to_string()))
        })
    });
    
    group.finish();
}

/// Test the resolver's string processing path
fn bench_resolver_string_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolver_string_path");
    
    let resolver = StandardTagResolver;
    let context = TagContext::new();
    
    // Test different string types through resolver
    let plain_string = YamlAst::PlainString("plain-text-no-interpolation".to_string());
    let handlebars_string = YamlAst::PlainString("{{variable}}".to_string());
    
    group.bench_function("resolver_plain_string", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&plain_string), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("resolver_handlebars_string", |b| {
        b.iter(|| {
            resolver.resolve_ast(black_box(&handlebars_string), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_string_processing_approaches,
    bench_resolver_string_path
);

criterion_main!(benches);