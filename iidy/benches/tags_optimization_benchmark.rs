//! Targeted benchmark for YAML tag resolution functions in tags.rs
//!
//! Benchmarks StandardTagResolver methods directly to measure optimization impact

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iidy::yaml::resolution::{TagContext, StandardTagResolver, TagResolver};
use iidy::yaml::engine::YamlPreprocessor;
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::parsing::ast::*;
use serde_yaml::Value;
use std::collections::HashMap;


/// Benchmark include tag resolution
fn bench_include_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("include_resolution");
    let resolver = StandardTagResolver;
    
    // Setup context with test data
    let mut context = TagContext::new();
    
    // Add nested data structure
    let mut config_data = serde_yaml::Mapping::new();
    config_data.insert(Value::String("host".to_string()), Value::String("localhost".to_string()));
    config_data.insert(Value::String("port".to_string()), Value::Number(serde_yaml::Number::from(8080)));
    
    let mut database_data = serde_yaml::Mapping::new();
    database_data.insert(Value::String("config".to_string()), Value::Mapping(config_data));
    
    context = context.with_variable("database", Value::Mapping(database_data));
    
    // Simple variable lookup
    let simple_include = IncludeTag {
        path: "database".to_string(),
        query: None,
    };
    
    group.bench_function("simple_lookup", |b| {
        b.iter(|| {
            resolver.resolve_include(black_box(&simple_include), black_box(&context)).unwrap()
        })
    });
    
    // Nested path lookup
    let nested_include = IncludeTag {
        path: "database.config.host".to_string(),
        query: None,
    };
    
    group.bench_function("nested_path", |b| {
        b.iter(|| {
            resolver.resolve_include(black_box(&nested_include), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark map tag resolution
fn bench_map_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("map_resolution");
    let tag_resolver = StandardTagResolver;
    let context = TagContext::new();
    
    // Small list
    let small_items = YamlAst::Sequence(vec![
        YamlAst::PlainString("api".to_string()),
        YamlAst::PlainString("web".to_string()),
        YamlAst::PlainString("worker".to_string()),
    ]);
    
    let small_map_tag = MapTag {
        items: Box::new(small_items),
        template: Box::new(YamlAst::PlainString("service-{{item}}".to_string())),
        var: Some("item".to_string()),
        filter: None,
    };
    
    group.bench_function("small_list", |b| {
        b.iter(|| {
            tag_resolver.resolve_map(black_box(&small_map_tag), black_box(&context)).unwrap()
        })
    });
    
    // Large list
    let large_items = YamlAst::Sequence((0..100).map(|i| YamlAst::PlainString(format!("item-{}", i))).collect());
    
    let large_map_tag = MapTag {
        items: Box::new(large_items),
        template: Box::new(YamlAst::PlainString("processed-{{item}}".to_string())),
        var: Some("item".to_string()),
        filter: None,
    };
    
    group.bench_function("large_list", |b| {
        b.iter(|| {
            tag_resolver.resolve_map(black_box(&large_map_tag), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark merge tag resolution
fn bench_merge_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_resolution");
    let tag_resolver = StandardTagResolver;
    let context = TagContext::new();
    
    // Simple merge
    let simple_merge_tag = MergeTag {
        sources: vec![
            YamlAst::Mapping(vec![
                (YamlAst::PlainString("name".to_string()), YamlAst::PlainString("app".to_string())),
                (YamlAst::PlainString("version".to_string()), YamlAst::PlainString("1.0".to_string())),
            ]),
            YamlAst::Mapping(vec![
                (YamlAst::PlainString("env".to_string()), YamlAst::PlainString("prod".to_string())),
                (YamlAst::PlainString("replicas".to_string()), YamlAst::Number(serde_yaml::Number::from(3))),
            ]),
        ],
    };
    
    group.bench_function("simple_merge", |b| {
        b.iter(|| {
            tag_resolver.resolve_merge(black_box(&simple_merge_tag), black_box(&context)).unwrap()
        })
    });
    
    // Complex merge with many sources
    let complex_sources: Vec<YamlAst> = (0..20).map(|i| {
        YamlAst::Mapping(vec![
            (YamlAst::PlainString(format!("key{}", i)), YamlAst::PlainString(format!("value{}", i))),
            (YamlAst::PlainString(format!("num{}", i)), YamlAst::Number(serde_yaml::Number::from(i))),
        ])
    }).collect();
    
    let complex_merge_tag = MergeTag {
        sources: complex_sources,
    };
    
    group.bench_function("complex_merge", |b| {
        b.iter(|| {
            tag_resolver.resolve_merge(black_box(&complex_merge_tag), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark string operations
fn bench_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");
    let tag_resolver = StandardTagResolver;
    let context = TagContext::new();
    
    // Join operation
    let join_tag = JoinTag {
        delimiter: Box::new(YamlAst::PlainString(",".to_string())),
        array: Box::new(YamlAst::Sequence(vec![
            YamlAst::PlainString("item1".to_string()),
            YamlAst::PlainString("item2".to_string()),
            YamlAst::PlainString("item3".to_string()),
            YamlAst::PlainString("item4".to_string()),
            YamlAst::PlainString("item5".to_string()),
        ])),
    };
    
    group.bench_function("join", |b| {
        b.iter(|| {
            tag_resolver.resolve_join(black_box(&join_tag), black_box(&context)).unwrap()
        })
    });
    
    // Split operation
    let split_tag = SplitTag {
        delimiter: Box::new(YamlAst::PlainString(",".to_string())),
        string: Box::new(YamlAst::PlainString("item1,item2,item3,item4,item5".to_string())),
    };
    
    group.bench_function("split", |b| {
        b.iter(|| {
            tag_resolver.resolve_split(black_box(&split_tag), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

/// Benchmark context operations
fn bench_context_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_operations");
    
    let base_context = TagContext::new();
    
    group.bench_function("with_small_bindings", |b| {
        b.iter(|| {
            let mut bindings = HashMap::new();
            bindings.insert("item".to_string(), Value::String("test".to_string()));
            bindings.insert("index".to_string(), Value::Number(serde_yaml::Number::from(0)));
            black_box(base_context.with_bindings(black_box(bindings)))
        })
    });
    
    group.bench_function("with_large_bindings", |b| {
        b.iter(|| {
            let mut bindings = HashMap::new();
            for i in 0..50 {
                bindings.insert(format!("var{}", i), Value::String(format!("value{}", i)));
            }
            black_box(base_context.with_bindings(black_box(bindings)))
        })
    });
    
    group.finish();
}

/// Benchmark resolve_ast_with_context (key orchestration function)
fn bench_resolve_ast_with_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve_ast_with_context");
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, true);
    
    let mut context = TagContext::new();
    context = context.with_variable("environment", Value::String("production".to_string()));
    context = context.with_variable("service_name", Value::String("api-server".to_string()));
    
    // Simple string with handlebars
    let simple_string_ast = YamlAst::PlainString("{{service_name}}-{{environment}}".to_string());
    
    group.bench_function("simple_string_interpolation", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(simple_string_ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Complex mapping with nested handlebars
    let complex_mapping_ast = YamlAst::Mapping(vec![
        (YamlAst::PlainString("name".to_string()), YamlAst::PlainString("{{service_name}}".to_string())),
        (YamlAst::PlainString("env".to_string()), YamlAst::PlainString("{{environment}}".to_string())),
        (YamlAst::PlainString("config".to_string()), YamlAst::Mapping(vec![
            (YamlAst::PlainString("host".to_string()), YamlAst::PlainString("{{service_name}}.{{environment}}.local".to_string())),
            (YamlAst::PlainString("port".to_string()), YamlAst::Number(serde_yaml::Number::from(8080))),
        ])),
    ]);
    
    group.bench_function("complex_mapping_interpolation", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(complex_mapping_ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    // Sequence with handlebars
    let sequence_ast = YamlAst::Sequence(vec![
        YamlAst::PlainString("{{service_name}}-worker-1".to_string()),
        YamlAst::PlainString("{{service_name}}-worker-2".to_string()),
        YamlAst::PlainString("{{service_name}}-worker-3".to_string()),
    ]);
    
    group.bench_function("sequence_interpolation", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(sequence_ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    // CloudFormation tag with handlebars
    let cfn_tag_ast = YamlAst::CloudFormationTag(CloudFormationTag::Sub(
        Box::new(YamlAst::PlainString("${AWS::StackName}-{{service_name}}-{{environment}}".to_string()))
    ));
    
    group.bench_function("cloudformation_tag_interpolation", |b| {
        b.iter(|| {
            preprocessor.resolve_ast_with_context(black_box(cfn_tag_ast.clone()), black_box(&context)).unwrap()
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_include_resolution,
    bench_map_resolution,
    bench_merge_resolution,
    bench_string_operations,
    bench_context_operations,
    bench_resolve_ast_with_context
);

criterion_main!(benches);

