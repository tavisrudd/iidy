//! Profiling binary for resolve_ast_with_context performance analysis

use iidy::yaml::preprocessor::YamlPreprocessor;
use iidy::yaml::tags::TagContext;
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::ast::*;
use serde_yaml::Value;
use std::time::Instant;

fn time_operation<F, R>(name: &str, op: F) -> R 
where 
    F: FnOnce() -> R 
{
    let start = Instant::now();
    let result = op();
    let duration = start.elapsed();
    println!("{}: {:?}", name, duration);
    result
}

fn main() {
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, true);
    
    let context = TagContext::new()
        .with_variable("service", Value::String("api-server".to_string()))
        .with_variable("environment", Value::String("production".to_string()))
        .with_variable("region", Value::String("us-west-2".to_string()));
    
    // Create a realistic CloudFormation-style template for profiling
    let template = YamlAst::Mapping(vec![
        (YamlAst::PlainString("AWSTemplateFormatVersion".to_string()), YamlAst::PlainString("2010-09-09".to_string())),
        (YamlAst::PlainString("Description".to_string()), YamlAst::TemplatedString("{{service}} deployment for {{environment}}".to_string())),
        (YamlAst::PlainString("Resources".to_string()), YamlAst::Mapping(vec![
            (YamlAst::PlainString("S3Bucket".to_string()), YamlAst::Mapping(vec![
                (YamlAst::PlainString("Type".to_string()), YamlAst::PlainString("AWS::S3::Bucket".to_string())),
                (YamlAst::PlainString("Properties".to_string()), YamlAst::Mapping(vec![
                    (YamlAst::PlainString("BucketName".to_string()), 
                     YamlAst::CloudFormationTag(CloudFormationTag::Sub(
                         Box::new(YamlAst::TemplatedString("${AWS::StackName}-{{service}}-{{environment}}".to_string()))
                     ))),
                    (YamlAst::PlainString("Tags".to_string()), YamlAst::Sequence(vec![
                        YamlAst::Mapping(vec![
                            (YamlAst::PlainString("Key".to_string()), YamlAst::PlainString("Service".to_string())),
                            (YamlAst::PlainString("Value".to_string()), YamlAst::TemplatedString("{{service}}".to_string())),
                        ]),
                        YamlAst::Mapping(vec![
                            (YamlAst::PlainString("Key".to_string()), YamlAst::PlainString("Environment".to_string())),
                            (YamlAst::PlainString("Value".to_string()), YamlAst::TemplatedString("{{environment}}".to_string())),
                        ]),
                    ])),
                ])),
            ])),
            (YamlAst::PlainString("LambdaFunction".to_string()), YamlAst::Mapping(vec![
                (YamlAst::PlainString("Type".to_string()), YamlAst::PlainString("AWS::Lambda::Function".to_string())),
                (YamlAst::PlainString("Properties".to_string()), YamlAst::Mapping(vec![
                    (YamlAst::PlainString("FunctionName".to_string()), 
                     YamlAst::CloudFormationTag(CloudFormationTag::Sub(
                         Box::new(YamlAst::TemplatedString("{{service}}-{{environment}}-function".to_string()))
                     ))),
                    (YamlAst::PlainString("Runtime".to_string()), YamlAst::PlainString("python3.9".to_string())),
                    (YamlAst::PlainString("Environment".to_string()), YamlAst::Mapping(vec![
                        (YamlAst::PlainString("Variables".to_string()), YamlAst::Mapping(vec![
                            (YamlAst::PlainString("SERVICE_NAME".to_string()), YamlAst::TemplatedString("{{service}}".to_string())),
                            (YamlAst::PlainString("ENVIRONMENT".to_string()), YamlAst::TemplatedString("{{environment}}".to_string())),
                            (YamlAst::PlainString("REGION".to_string()), YamlAst::TemplatedString("{{region}}".to_string())),
                        ])),
                    ])),
                ])),
            ])),
        ])),
    ]);
    
    println!("=== Cost Breakdown Analysis ===");
    
    // Test different AST node types to understand costs
    
    // 1. Plain string (no handlebars)
    let plain_string = YamlAst::PlainString("plain-text".to_string());
    time_operation("Plain string (1000x)", || {
        for _ in 0..1000 {
            let _ = preprocessor.resolve_ast_with_context(plain_string.clone(), &context).unwrap();
        }
    });
    
    // 2. String with handlebars
    let handlebars_string = YamlAst::TemplatedString("{{service}}-{{environment}}".to_string());
    time_operation("Handlebars string (1000x)", || {
        for _ in 0..1000 {
            let _ = preprocessor.resolve_ast_with_context(handlebars_string.clone(), &context).unwrap();
        }
    });
    
    // 3. Small mapping
    let small_mapping = YamlAst::Mapping(vec![
        (YamlAst::PlainString("name".to_string()), YamlAst::TemplatedString("{{service}}".to_string())),
        (YamlAst::PlainString("env".to_string()), YamlAst::TemplatedString("{{environment}}".to_string())),
    ]);
    time_operation("Small mapping (1000x)", || {
        for _ in 0..1000 {
            let _ = preprocessor.resolve_ast_with_context(small_mapping.clone(), &context).unwrap();
        }
    });
    
    // 4. CloudFormation tag
    let cfn_tag = YamlAst::CloudFormationTag(CloudFormationTag::Sub(
        Box::new(YamlAst::TemplatedString("${AWS::StackName}-{{service}}".to_string()))
    ));
    time_operation("CloudFormation tag (1000x)", || {
        for _ in 0..1000 {
            let _ = preprocessor.resolve_ast_with_context(cfn_tag.clone(), &context).unwrap();
        }
    });
    
    // 5. Complex realistic template
    time_operation("Complex template (100x)", || {
        for _ in 0..100 {
            let _ = preprocessor.resolve_ast_with_context(template.clone(), &context).unwrap();
        }
    });
    
    println!("\n=== Memory Allocation Analysis ===");
    
    // Test with varying mapping sizes to understand allocation costs
    for size in [2, 5, 10, 20, 50].iter() {
        let mut pairs = Vec::new();
        for i in 0..*size {
            pairs.push((
                YamlAst::PlainString(format!("key_{}", i)),
                YamlAst::TemplatedString(format!("{{{{service}}}}_value_{}", i))
            ));
        }
        let mapping = YamlAst::Mapping(pairs);
        
        time_operation(&format!("Mapping size {} (100x)", size), || {
            for _ in 0..100 {
                let _ = preprocessor.resolve_ast_with_context(mapping.clone(), &context).unwrap();
            }
        });
    }
    
    println!("Analysis complete!");
}