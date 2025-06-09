//! Realistic string processing benchmark for YAML preprocessing
//!
//! Tests batch processing of various string lengths and patterns
//! that represent real-world YAML content.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iidy::yaml::preprocessor::YamlPreprocessor;
use iidy::yaml::tags::TagContext;
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::ast::YamlAst;
use serde_yaml::Value;

fn bench_realistic_string_scenarios(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_string_scenarios");
    
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, true);
    let context = TagContext::new()
        .with_variable("service", Value::String("api-server".to_string()))
        .with_variable("env", Value::String("production".to_string()));
    
    // Scenario 1: Mixed list of short to medium strings (realistic YAML content)
    let mixed_string_list = YamlAst::Sequence(vec![
        YamlAst::PlainString("app".to_string()),                                    // 3 chars
        YamlAst::PlainString("production".to_string()),                             // 10 chars  
        YamlAst::PlainString("us-west-2".to_string()),                             // 9 chars
        YamlAst::PlainString("api-server-v1.2.3".to_string()),                     // 17 chars
        YamlAst::PlainString("arn:aws:s3:::my-bucket".to_string()),                // 22 chars
        YamlAst::PlainString("application/json".to_string()),                       // 16 chars
        YamlAst::PlainString("2023-10-15T10:30:00Z".to_string()),                  // 20 chars (timestamp)
        YamlAst::PlainString("10.0.1.0/24".to_string()),                           // 12 chars (CIDR)
        YamlAst::PlainString("sha256:abc123def456".to_string()),                    // 20 chars (hash)
        YamlAst::PlainString("tcp".to_string()),                                    // 3 chars
    ]);
    
    group.bench_function("mixed_string_list_10_items", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(mixed_string_list.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Scenario 2: Configuration-style mapping with varied string lengths
    let config_mapping = YamlAst::Mapping(vec![
        (YamlAst::PlainString("app_name".to_string()), YamlAst::PlainString("my-service".to_string())),
        (YamlAst::PlainString("version".to_string()), YamlAst::PlainString("1.2.3".to_string())),
        (YamlAst::PlainString("description".to_string()), YamlAst::PlainString("A high-performance API service for data processing".to_string())),
        (YamlAst::PlainString("region".to_string()), YamlAst::PlainString("us-west-2".to_string())),
        (YamlAst::PlainString("environment".to_string()), YamlAst::PlainString("production".to_string())),
        (YamlAst::PlainString("image".to_string()), YamlAst::PlainString("registry.company.com/my-service:1.2.3".to_string())),
        (YamlAst::PlainString("port".to_string()), YamlAst::Number(serde_yaml::Number::from(8080))),
        (YamlAst::PlainString("health_check_path".to_string()), YamlAst::PlainString("/health".to_string())),
    ]);
    
    group.bench_function("config_mapping_mixed_types", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(config_mapping.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Scenario 3: Large strings that should NOT be considered simple
    let large_strings = YamlAst::Sequence(vec![
        YamlAst::PlainString("This is a very long string that represents documentation or configuration that might contain extensive details about the application setup, deployment procedures, troubleshooting guides, and other operational information that teams need to maintain and understand the system".to_string()), // ~300 chars
        YamlAst::PlainString("Another long configuration value that might contain JSON-like data or XML content or other structured information that applications commonly store in YAML files for configuration management and deployment automation across different environments".to_string()), // ~250 chars
        YamlAst::PlainString("Short".to_string()),
        YamlAst::PlainString("Medium length string with some details".to_string()),
    ]);
    
    group.bench_function("mixed_with_large_strings", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(large_strings.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Scenario 4: Strings with handlebars (should go to slow path)
    let templated_strings = YamlAst::Sequence(vec![
        YamlAst::PlainString("plain-string".to_string()),
        YamlAst::TemplatedString("{{service}}-worker".to_string()),
        YamlAst::PlainString("another-plain-string".to_string()),
        YamlAst::TemplatedString("{{env}}-database".to_string()),
        YamlAst::PlainString("static-value".to_string()),
    ]);
    
    group.bench_function("mixed_templated_strings", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(templated_strings.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Scenario 5: Single string comparisons by length
    let short_string = YamlAst::PlainString("app".to_string());
    let medium_string = YamlAst::PlainString("my-application-service".to_string());
    let long_string = YamlAst::PlainString("This is a longer string that might represent a description or documentation that applications commonly include in their YAML configuration files".to_string());
    let very_long_string = YamlAst::PlainString("This is an extremely long string that represents the kind of extensive documentation, configuration details, or other verbose content that sometimes appears in YAML files and should probably not be considered for fast path optimization because the overhead of checking and processing such long strings might outweigh the benefits of the optimization".to_string());
    
    group.bench_function("short_string_3_chars", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(short_string.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("medium_string_23_chars", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(medium_string.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("long_string_150_chars", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(long_string.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.bench_function("very_long_string_400_chars", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(very_long_string.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Test the effect of string length thresholds on optimization
fn bench_string_length_thresholds(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_length_analysis");
    
    // Test different string lengths to find optimal threshold
    let test_strings = vec![
        ("10_chars", "short_test".to_string()),
        ("20_chars", "medium_length_string".to_string()), 
        ("50_chars", "this_is_a_somewhat_longer_string_for_testing_perf".to_string()),
        ("100_chars", "this_is_a_much_longer_string_that_we_use_to_test_the_performance_characteristics_of_our_optimization".to_string()),
        ("200_chars", "this_is_an_even_longer_string_that_represents_the_kind_of_verbose_content_that_might_appear_in_yaml_files_and_we_want_to_understand_how_our_optimization_performs_with_such_content_in_realistic_scenarios".to_string()),
    ];
    
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, true);
    let context = TagContext::new();
    
    for (name, content) in test_strings {
        let ast = YamlAst::PlainString(content);
        group.bench_function(name, |b| {
            b.iter(|| {
                preprocessor.resolve_ast_with_context(black_box(ast.clone()), black_box(&context)).unwrap()
            })
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_realistic_string_scenarios,
    bench_string_length_thresholds
);

criterion_main!(benches);