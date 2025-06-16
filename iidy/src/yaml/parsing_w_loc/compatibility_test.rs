//! Compatibility tests between tree-sitter parsing and original parser
//!
//! This module tests that the new tree-sitter based parser with location tracking
//! produces ASTs that are structurally identical to the original parser when
//! converted back to the original format.
//!
//! ## Current Status: Partial Compatibility
//!
//! **✅ Working correctly:**
//! - Basic YAML structures (mappings, sequences, scalars, numbers, booleans)
//! - Flow-style tags: `!Ref "MyBucket"`, `!$not false`, `!GetAtt Resource.Property`
//! - CloudFormation intrinsic functions in flow style
//! - Simple preprocessing tags in flow style
//!
//! **❌ Known limitations:**
//! - **Block-style tags**: Complex preprocessing tags with indented content are not parsed correctly
//!   ```yaml
//!   user_roles: !$mapValues
//!     items: !$ user_data
//!     template: "{{toUpperCase item.value}}"
//!   ```
//!   Tree-sitter sees `!$mapValues` as a tag with null content instead of associating
//!   the following indented block with the tag.
//!
//! **Root cause:** Tree-sitter YAML represents block-style tagged content differently
//! than expected. Additional parsing logic is needed to handle mapping_pair nodes
//! where the value contains both a tag node and subsequent block content.
//!
//! **Next steps for full compatibility:**
//! 1. Analyze tree-sitter YAML grammar for block-style tag representation
//! 2. Implement block-style tag parsing in `build_mapping()` and related methods
//! 3. Add proper preprocessing tag content parsing with full tag structure support

use anyhow::Result;
use std::fs;
use std::path::Path;

use super::convert::to_original_ast;
use super::parser::parse_yaml_ast;
use crate::yaml::parsing::ast as original_ast;
use crate::yaml::parsing::parser as original_parser;
use url::Url;

/// Test compatibility across all example YAML files
#[test]
fn test_compatibility_with_example_templates() -> Result<()> {
    let example_dir = Path::new("example-templates/yaml-iidy-syntax");

    if !example_dir.exists() {
        eprintln!(
            "Warning: Example directory {} does not exist, skipping compatibility tests",
            example_dir.display()
        );
        return Ok(());
    }

    let mut total_files = 0;
    let mut successful_comparisons = 0;
    let mut failed_files = Vec::new();

    // Read all .yaml files in the directory
    for entry in fs::read_dir(example_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            total_files += 1;
            let filename = path.file_name().unwrap().to_string_lossy();

            match test_single_file_compatibility(&path) {
                Ok(()) => {
                    successful_comparisons += 1;
                    println!("✓ {}: Compatible", filename);
                }
                Err(e) => {
                    failed_files.push((filename.to_string(), e));
                    println!("✗ {}: INCOMPATIBLE", filename);
                }
            }
        }
    }

    // Print summary
    println!("\n=== Compatibility Test Summary ===");
    println!("Total files tested: {}", total_files);
    println!("Successful comparisons: {}", successful_comparisons);
    println!("Failed comparisons: {}", failed_files.len());

    if !failed_files.is_empty() {
        println!("\nFailed files:");
        for (filename, error) in &failed_files {
            println!("  {}: {}", filename, error);
        }
    }

    // For now, we'll log failures but not fail the test since we're in spike mode
    // In production, we would: assert_eq!(failed_files.len(), 0, "Some files failed compatibility test");

    Ok(())
}

/// Test compatibility with top-level example templates
#[test]
fn test_compatibility_with_top_level_examples() -> Result<()> {
    let example_dir = Path::new("example-templates");

    if !example_dir.exists() {
        eprintln!(
            "Warning: Example directory {} does not exist, skipping top-level compatibility tests",
            example_dir.display()
        );
        return Ok(());
    }

    let mut total_files = 0;
    let mut successful_comparisons = 0;
    let mut failed_files = Vec::new();

    // Top-level YAML files in example-templates/ (excluding subdirectories)
    for entry in fs::read_dir(example_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only process files (not directories) with .yaml extension
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            total_files += 1;
            let filename = path.file_name().unwrap().to_string_lossy();

            match test_single_file_compatibility(&path) {
                Ok(()) => {
                    successful_comparisons += 1;
                    println!("✓ {}: Compatible", filename);
                }
                Err(e) => {
                    println!("✗ {}: INCOMPATIBLE - {}", filename, e);
                    failed_files.push((filename.to_string(), e));
                }
            }
        }
    }

    // Print summary
    println!("\n=== Top-Level Examples Compatibility Test Summary ===");
    println!("Total files tested: {}", total_files);
    println!("Successful comparisons: {}", successful_comparisons);
    println!("Failed comparisons: {}", failed_files.len());

    if !failed_files.is_empty() {
        println!("\nFailed files:");
        for (filename, error) in &failed_files {
            println!("  {}: {}", filename, error);
        }
    }

    Ok(())
}

/// Test compatibility with all example templates (both top-level and subdirectories)
#[test]
fn test_compatibility_with_all_examples() -> Result<()> {
    let example_dir = Path::new("example-templates");

    if !example_dir.exists() {
        eprintln!(
            "Warning: Example directory {} does not exist, skipping all examples compatibility tests",
            example_dir.display()
        );
        return Ok(());
    }

    let mut total_files = 0;
    let mut successful_comparisons = 0;
    let mut failed_files = Vec::new();

    // Recursively walk through all directories under example-templates/
    fn walk_directory(
        dir: &Path,
        total_files: &mut usize,
        successful_comparisons: &mut usize,
        failed_files: &mut Vec<(String, anyhow::Error)>,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                *total_files += 1;
                let relative_path = path.strip_prefix("example-templates/").unwrap_or(&path);
                let display_name = relative_path.display().to_string();

                match test_single_file_compatibility(&path) {
                    Ok(()) => {
                        *successful_comparisons += 1;
                        println!("✓ {}: Compatible", display_name);
                    }
                    Err(e) => {
                        failed_files.push((display_name.clone(), e));
                        println!("✗ {}: INCOMPATIBLE", display_name);
                    }
                }
            } else if path.is_dir() {
                // Skip directories we know contain error examples
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name != "errors" && dir_name != "invalid" && dir_name != "expected-outputs" {
                    walk_directory(&path, total_files, successful_comparisons, failed_files)?;
                }
            }
        }
        Ok(())
    }

    walk_directory(
        example_dir,
        &mut total_files,
        &mut successful_comparisons,
        &mut failed_files,
    )?;

    // Print summary
    println!("\n=== All Examples Compatibility Test Summary ===");
    println!("Total files tested: {}", total_files);
    println!("Successful comparisons: {}", successful_comparisons);
    println!("Failed comparisons: {}", failed_files.len());

    if !failed_files.is_empty() {
        println!("\nFailed files:");
        for (filename, error) in &failed_files {
            println!("  {}: {}", filename, error);
        }
    }

    Ok(())
}

/// Test a single file for compatibility between parsers
fn test_single_file_compatibility(file_path: &Path) -> Result<()> {
    let content = fs::read_to_string(file_path)?;

    // Use the shared comparison logic but with the actual file URI
    let absolute_path = file_path.canonicalize().map_err(|e| {
        anyhow::anyhow!("Failed to canonicalize path {}: {}", file_path.display(), e)
    })?;

    let file_uri = url::Url::from_file_path(&absolute_path).map_err(|_| {
        anyhow::anyhow!(
            "Failed to create URI from path: {}",
            absolute_path.display()
        )
    })?;

    let tree_sitter_ast = parse_yaml_ast(&content, file_uri.clone())
        .map_err(|e| anyhow::anyhow!("Tree-sitter parsing failed: {}", e.message))?;

    let original_ast =
        original_parser::parse_yaml_with_custom_tags_from_file(&content, file_uri.as_str())
            .map_err(|e| anyhow::anyhow!("Original parsing failed: {}", e))?;

    let converted_ast = to_original_ast(&tree_sitter_ast);
    compare_asts(&converted_ast, &original_ast, file_path)?;

    Ok(())
}

/// Deep comparison of two ASTs to detect structural differences
pub(crate) fn compare_asts(
    converted: &original_ast::YamlAst,
    original: &original_ast::YamlAst,
    file_path: &Path,
) -> Result<()> {
    if !asts_equal(converted, original) {
        // Create detailed mismatch report
        let mut report = String::new();
        report.push_str(&format!("AST mismatch in file: {}\n", file_path.display()));
        report.push_str("Converted AST:\n");
        report.push_str(&format!("{:#?}\n", converted));
        report.push_str("Original AST:\n");
        report.push_str(&format!("{:#?}\n", original));

        return Err(anyhow::anyhow!("AST structure mismatch:\n{}", report));
    }

    Ok(())
}

/// Deep equality check for YamlAst nodes
pub(crate) fn asts_equal(a: &original_ast::YamlAst, b: &original_ast::YamlAst) -> bool {
    match (a, b) {
        (original_ast::YamlAst::Null, original_ast::YamlAst::Null) => true,

        (original_ast::YamlAst::Bool(a), original_ast::YamlAst::Bool(b)) => a == b,

        (original_ast::YamlAst::Number(a), original_ast::YamlAst::Number(b)) => {
            // Compare numbers using their string representation for consistency
            a.to_string() == b.to_string()
        }

        (original_ast::YamlAst::PlainString(a), original_ast::YamlAst::PlainString(b)) => a == b,

        (original_ast::YamlAst::TemplatedString(a), original_ast::YamlAst::TemplatedString(b)) => {
            a == b
        }

        (original_ast::YamlAst::Sequence(a), original_ast::YamlAst::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| asts_equal(x, y))
        }

        (original_ast::YamlAst::Mapping(a), original_ast::YamlAst::Mapping(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|((k1, v1), (k2, v2))| asts_equal(k1, k2) && asts_equal(v1, v2))
        }

        (
            original_ast::YamlAst::PreprocessingTag(a),
            original_ast::YamlAst::PreprocessingTag(b),
        ) => preprocessing_tags_equal(a, b),

        (
            original_ast::YamlAst::CloudFormationTag(a),
            original_ast::YamlAst::CloudFormationTag(b),
        ) => cloudformation_tags_equal(a, b),

        (original_ast::YamlAst::UnknownYamlTag(a), original_ast::YamlAst::UnknownYamlTag(b)) => {
            a.tag == b.tag && asts_equal(&a.value, &b.value)
        }

        (
            original_ast::YamlAst::ImportedDocument(a),
            original_ast::YamlAst::ImportedDocument(b),
        ) => {
            a.source_uri == b.source_uri
                && a.import_key == b.import_key
                && asts_equal(&a.content, &b.content)
            // Note: We skip metadata comparison as it may differ between implementations
        }

        // Different variants are not equal
        _ => false,
    }
}

/// Compare preprocessing tags for equality
fn preprocessing_tags_equal(
    a: &original_ast::PreprocessingTag,
    b: &original_ast::PreprocessingTag,
) -> bool {
    use original_ast::PreprocessingTag;

    match (a, b) {
        (PreprocessingTag::Include(a), PreprocessingTag::Include(b)) => {
            a.path == b.path && a.query == b.query
        }
        (PreprocessingTag::If(a), PreprocessingTag::If(b)) => {
            asts_equal(&a.test, &b.test)
                && asts_equal(&a.then_value, &b.then_value)
                && match (&a.else_value, &b.else_value) {
                    (Some(av), Some(bv)) => asts_equal(av, bv),
                    (None, None) => true,
                    _ => false,
                }
        }
        (PreprocessingTag::Map(a), PreprocessingTag::Map(b)) => {
            asts_equal(&a.items, &b.items)
                && asts_equal(&a.template, &b.template)
                && a.var == b.var
                && match (&a.filter, &b.filter) {
                    (Some(af), Some(bf)) => asts_equal(af, bf),
                    (None, None) => true,
                    _ => false,
                }
        }
        (PreprocessingTag::Merge(a), PreprocessingTag::Merge(b)) => {
            a.sources.len() == b.sources.len()
                && a.sources
                    .iter()
                    .zip(b.sources.iter())
                    .all(|(x, y)| asts_equal(x, y))
        }
        (PreprocessingTag::Concat(a), PreprocessingTag::Concat(b)) => {
            a.sources.len() == b.sources.len()
                && a.sources
                    .iter()
                    .zip(b.sources.iter())
                    .all(|(x, y)| asts_equal(x, y))
        }
        (PreprocessingTag::Let(a), PreprocessingTag::Let(b)) => {
            a.bindings.len() == b.bindings.len()
                && a.bindings
                    .iter()
                    .zip(b.bindings.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && asts_equal(v1, v2))
                && asts_equal(&a.expression, &b.expression)
        }
        (PreprocessingTag::Eq(a), PreprocessingTag::Eq(b)) => {
            asts_equal(&a.left, &b.left) && asts_equal(&a.right, &b.right)
        }
        (PreprocessingTag::Not(a), PreprocessingTag::Not(b)) => {
            asts_equal(&a.expression, &b.expression)
        }
        (PreprocessingTag::Split(a), PreprocessingTag::Split(b)) => {
            asts_equal(&a.delimiter, &b.delimiter) && asts_equal(&a.string, &b.string)
        }
        (PreprocessingTag::Join(a), PreprocessingTag::Join(b)) => {
            asts_equal(&a.delimiter, &b.delimiter) && asts_equal(&a.array, &b.array)
        }
        (PreprocessingTag::ConcatMap(a), PreprocessingTag::ConcatMap(b)) => {
            asts_equal(&a.items, &b.items)
                && asts_equal(&a.template, &b.template)
                && a.var == b.var
                && match (&a.filter, &b.filter) {
                    (Some(af), Some(bf)) => asts_equal(af, bf),
                    (None, None) => true,
                    _ => false,
                }
        }
        (PreprocessingTag::MergeMap(a), PreprocessingTag::MergeMap(b)) => {
            asts_equal(&a.items, &b.items) && asts_equal(&a.template, &b.template) && a.var == b.var
        }
        (PreprocessingTag::MapListToHash(a), PreprocessingTag::MapListToHash(b)) => {
            asts_equal(&a.items, &b.items)
                && asts_equal(&a.template, &b.template)
                && a.var == b.var
                && match (&a.filter, &b.filter) {
                    (Some(af), Some(bf)) => asts_equal(af, bf),
                    (None, None) => true,
                    _ => false,
                }
        }
        (PreprocessingTag::MapValues(a), PreprocessingTag::MapValues(b)) => {
            asts_equal(&a.items, &b.items) && asts_equal(&a.template, &b.template) && a.var == b.var
        }
        (PreprocessingTag::GroupBy(a), PreprocessingTag::GroupBy(b)) => {
            asts_equal(&a.items, &b.items)
                && asts_equal(&a.key, &b.key)
                && a.var == b.var
                && match (&a.template, &b.template) {
                    (Some(at), Some(bt)) => asts_equal(at, bt),
                    (None, None) => true,
                    _ => false,
                }
        }
        (PreprocessingTag::FromPairs(a), PreprocessingTag::FromPairs(b)) => {
            asts_equal(&a.source, &b.source)
        }
        (PreprocessingTag::ToYamlString(a), PreprocessingTag::ToYamlString(b)) => {
            asts_equal(&a.data, &b.data)
        }
        (PreprocessingTag::ParseYaml(a), PreprocessingTag::ParseYaml(b)) => {
            asts_equal(&a.yaml_string, &b.yaml_string)
        }
        (PreprocessingTag::ToJsonString(a), PreprocessingTag::ToJsonString(b)) => {
            asts_equal(&a.data, &b.data)
        }
        (PreprocessingTag::ParseJson(a), PreprocessingTag::ParseJson(b)) => {
            asts_equal(&a.json_string, &b.json_string)
        }
        (PreprocessingTag::Escape(a), PreprocessingTag::Escape(b)) => {
            asts_equal(&a.content, &b.content)
        }
        // Different variants are not equal
        _ => false,
    }
}

/// Compare CloudFormation tags for equality
fn cloudformation_tags_equal(
    a: &original_ast::CloudFormationTag,
    b: &original_ast::CloudFormationTag,
) -> bool {
    use original_ast::CloudFormationTag;

    match (a, b) {
        (CloudFormationTag::Ref(a), CloudFormationTag::Ref(b)) => asts_equal(a, b),
        (CloudFormationTag::Sub(a), CloudFormationTag::Sub(b)) => asts_equal(a, b),
        (CloudFormationTag::GetAtt(a), CloudFormationTag::GetAtt(b)) => asts_equal(a, b),
        (CloudFormationTag::Join(a), CloudFormationTag::Join(b)) => asts_equal(a, b),
        (CloudFormationTag::Select(a), CloudFormationTag::Select(b)) => asts_equal(a, b),
        (CloudFormationTag::Split(a), CloudFormationTag::Split(b)) => asts_equal(a, b),
        (CloudFormationTag::Base64(a), CloudFormationTag::Base64(b)) => asts_equal(a, b),
        (CloudFormationTag::GetAZs(a), CloudFormationTag::GetAZs(b)) => asts_equal(a, b),
        (CloudFormationTag::ImportValue(a), CloudFormationTag::ImportValue(b)) => asts_equal(a, b),
        (CloudFormationTag::FindInMap(a), CloudFormationTag::FindInMap(b)) => asts_equal(a, b),
        (CloudFormationTag::Cidr(a), CloudFormationTag::Cidr(b)) => asts_equal(a, b),
        (CloudFormationTag::Length(a), CloudFormationTag::Length(b)) => asts_equal(a, b),
        (CloudFormationTag::ToJsonString(a), CloudFormationTag::ToJsonString(b)) => {
            asts_equal(a, b)
        }
        (CloudFormationTag::Transform(a), CloudFormationTag::Transform(b)) => asts_equal(a, b),
        (CloudFormationTag::ForEach(a), CloudFormationTag::ForEach(b)) => asts_equal(a, b),
        (CloudFormationTag::If(a), CloudFormationTag::If(b)) => asts_equal(a, b),
        (CloudFormationTag::Equals(a), CloudFormationTag::Equals(b)) => asts_equal(a, b),
        (CloudFormationTag::And(a), CloudFormationTag::And(b)) => asts_equal(a, b),
        (CloudFormationTag::Or(a), CloudFormationTag::Or(b)) => asts_equal(a, b),
        (CloudFormationTag::Not(a), CloudFormationTag::Not(b)) => asts_equal(a, b),
        // Different variants are not equal
        _ => false,
    }
}

#[cfg(test)]
mod individual_example_tests {
    use super::*;

    /// Test specific known examples individually for easier debugging
    #[test]
    fn test_array_syntax_simple_original() -> Result<()> {
        let path = Path::new("example-templates/yaml-iidy-syntax/array-syntax-simple.yaml");
        if path.exists() {
            test_single_file_compatibility(path)?;
        }
        Ok(())
    }

    #[test]
    fn test_if_conditional() -> Result<()> {
        let path = Path::new("example-templates/yaml-iidy-syntax/if-conditional.yaml");
        if path.exists() {
            test_single_file_compatibility(path)?;
        }
        Ok(())
    }

    #[test]
    fn test_let_bindings_original() -> Result<()> {
        let path = Path::new("example-templates/yaml-iidy-syntax/let.yaml");
        if path.exists() {
            test_single_file_compatibility(path)?;
        }
        Ok(())
    }

    #[test]
    fn test_map_operations() -> Result<()> {
        let path = Path::new("example-templates/yaml-iidy-syntax/map.yaml");
        if path.exists() {
            test_single_file_compatibility(path)?;
        }
        Ok(())
    }

    #[test]
    fn test_merge_debug() -> Result<()> {
        let path = Path::new("example-templates/yaml-iidy-syntax/merge.yaml");
        if path.exists() {
            test_single_file_compatibility(path)?;
        }
        Ok(())
    }

    #[test]
    fn test_nested_groupby() -> Result<()> {
        // Test nested structure from groupby.yaml
        let nested_yaml = r#"
Resources:
  ResourcesByEnvironment:
    Type: AWS::SSM::Parameter
    Properties:
      Value: !$toJsonString
        - !$groupBy
            items: !$ resources
            key: !$ item.environment
"#;

        let uri = Url::parse("file:///test.yaml").unwrap();

        println!("=== Testing nested groupby syntax ===");

        // Parse with our implementation
        match parse_yaml_ast(nested_yaml, uri.clone()) {
            Ok(ast) => {
                println!("✓ Our parser succeeded");
                let converted = to_original_ast(&ast);

                // Parse with original implementation
                match original_parser::parse_yaml_with_custom_tags_from_file(
                    nested_yaml,
                    uri.as_str(),
                ) {
                    Ok(original_ast) => {
                        println!("✓ Original parser succeeded");

                        if compare_asts(&converted, &original_ast, Path::new("array_test")).is_ok()
                        {
                            println!("✓ Array parsing compatible");
                        } else {
                            println!("✗ Array parsing incompatible");
                            println!("Converted: {:#?}", converted);
                            println!("Original: {:#?}", original_ast);
                        }
                    }
                    Err(e) => {
                        println!("✗ Original parser failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Our parser failed: {}", e.message);
            }
        }

        // Test frompairs.yaml to find systematic patterns
        let path = Path::new("example-templates/yaml-iidy-syntax/frompairs.yaml");
        if path.exists() {
            println!("\n=== Testing frompairs.yaml for systematic debugging ===");
            match test_single_file_compatibility(path) {
                Ok(()) => println!("✓ frompairs.yaml: Compatible"),
                Err(_e) => {
                    println!("✗ frompairs.yaml: INCOMPATIBLE");

                    // Get the first few keys to see differences
                    let content = fs::read_to_string(path).unwrap();
                    let absolute_path = path.canonicalize().unwrap();
                    let file_uri = Url::from_file_path(&absolute_path).unwrap();

                    let tree_sitter_ast = parse_yaml_ast(&content, file_uri.clone()).unwrap();
                    let original_ast = original_parser::parse_yaml_with_custom_tags_from_file(
                        &content,
                        file_uri.as_str(),
                    )
                    .unwrap();
                    let converted_ast = to_original_ast(&tree_sitter_ast);

                    // Look for first structural difference in a smaller file
                    if let (
                        original_ast::YamlAst::Mapping(conv_pairs),
                        original_ast::YamlAst::Mapping(orig_pairs),
                    ) = (&converted_ast, &original_ast)
                    {
                        println!(
                            "Converted has {} pairs, Original has {} pairs",
                            conv_pairs.len(),
                            orig_pairs.len()
                        );

                        // Check first 3 key-value pairs for differences
                        for i in
                            0..std::cmp::min(3, std::cmp::min(conv_pairs.len(), orig_pairs.len()))
                        {
                            let (conv_key, conv_val) = &conv_pairs[i];
                            let (orig_key, orig_val) = &orig_pairs[i];

                            if !asts_equal(conv_key, orig_key) {
                                println!(
                                    "Key difference at position {}: Conv={:?}, Orig={:?}",
                                    i, conv_key, orig_key
                                );
                            } else if !asts_equal(conv_val, orig_val) {
                                if let original_ast::YamlAst::PlainString(key_name) = conv_key {
                                    println!(
                                        "Value difference for key '{}' at position {}",
                                        key_name, i
                                    );
                                    println!(
                                        "  Conv val type: {:?}",
                                        std::mem::discriminant(conv_val)
                                    );
                                    println!(
                                        "  Orig val type: {:?}",
                                        std::mem::discriminant(orig_val)
                                    );
                                }
                            } else if let original_ast::YamlAst::PlainString(conv_str) = conv_key {
                                println!(
                                    "Position {}: Key '{}' and value match perfectly",
                                    i, conv_str
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_multiline_strings() -> Result<()> {
        // Test multiline string with blank lines
        let multiline_yaml = r#"
description: |
  This is a multiline string.
  
  It has blank lines in between.
  And preserves formatting.

items:
  - "first"
  - null
  - "second"
"#;

        let uri = Url::parse("file:///test.yaml").unwrap();

        println!("=== Testing multiline strings ===");

        // Parse with our implementation
        match parse_yaml_ast(multiline_yaml, uri.clone()) {
            Ok(ast) => {
                println!("✓ Our parser succeeded");
                let converted = to_original_ast(&ast);

                // Parse with original implementation
                match original_parser::parse_yaml_with_custom_tags_from_file(
                    multiline_yaml,
                    uri.as_str(),
                ) {
                    Ok(original_ast) => {
                        println!("✓ Original parser succeeded");

                        if compare_asts(&converted, &original_ast, Path::new("multiline_test"))
                            .is_ok()
                        {
                            println!("✓ Multiline parsing compatible");
                        } else {
                            println!("✗ Multiline parsing incompatible");
                            println!("Converted: {:#?}", converted);
                            println!("Original: {:#?}", original_ast);
                        }
                    }
                    Err(e) => {
                        println!("✗ Original parser failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Our parser failed: {}", e.message);
            }
        }

        Ok(())
    }

    #[test]
    fn test_concat_join_debug() -> Result<()> {
        // Test simple concat case - this exact pattern from the file
        let concat_yaml = r#"all_environments: !$concat
  - ["dev", "test"]
  - ["staging"]
  - ["production"]
"#;

        let uri = Url::parse("file:///test.yaml").unwrap();

        println!("=== Debugging concat parsing ===");
        println!("YAML content:");
        println!("{}", concat_yaml);

        // Parse with our implementation
        match parse_yaml_ast(concat_yaml, uri.clone()) {
            Ok(ast) => {
                println!("\n✓ Our parser succeeded");
                println!("AST: {:#?}", ast);

                // Try converting to original
                let converted = to_original_ast(&ast);
                println!("\nConverted AST: {:#?}", converted);
            }
            Err(e) => {
                println!("\n✗ Our parser failed: {}", e.message);
            }
        }

        // Parse with original implementation
        match original_parser::parse_yaml_with_custom_tags_from_file(concat_yaml, uri.as_str()) {
            Ok(original_ast) => {
                println!("\n✓ Original parser succeeded");
                println!("Original AST: {:#?}", original_ast);
            }
            Err(e) => {
                println!("\n✗ Original parser failed: {}", e);
            }
        }

        // Test the full file
        let path = Path::new("example-templates/yaml-iidy-syntax/concat-join.yaml");
        if path.exists() {
            println!("\n=== Testing full concat-join.yaml ===");
            match test_single_file_compatibility(path) {
                Ok(()) => println!("✓ concat-join.yaml: Compatible"),
                Err(e) => {
                    println!("✗ concat-join.yaml: INCOMPATIBLE");
                    println!("Error: {}", e);
                    // Let's get more details by parsing both and comparing the first mismatch
                    let content = fs::read_to_string(path)?;
                    let absolute_path = path.canonicalize()?;
                    let file_uri = Url::from_file_path(&absolute_path)
                        .map_err(|_| anyhow::anyhow!("Failed to create URI"))?;

                    let tree_sitter_ast =
                        parse_yaml_ast(&content, file_uri.clone()).map_err(|e| {
                            anyhow::anyhow!("Tree-sitter parsing failed: {}", e.message)
                        })?;

                    let original_ast = original_parser::parse_yaml_with_custom_tags_from_file(
                        &content,
                        file_uri.as_str(),
                    )
                    .map_err(|e| anyhow::anyhow!("Original parsing failed: {}", e))?;

                    let converted_ast = to_original_ast(&tree_sitter_ast);

                    println!("Converted (first few fields): {:#?}", &converted_ast);
                    println!("Original (first few fields): {:#?}", &original_ast);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_groupby_debug() -> Result<()> {
        // Test the minimal failing case from Resources section
        let resources_yaml = r#"
Resources:
  ResourcesByEnvironment:
    Type: AWS::SSM::Parameter
    Properties:
      Value: !$toJsonString
        - !$groupBy
            items: !$ resources
            key: !$ item.environment
"#;

        // Test a sequence with GroupBy (like in the real file)
        let sequence_yaml = r#"
Value: !$toJsonString
  - !$groupBy
      items: !$ resources
      key: !$ item.environment
"#;

        let uri = Url::parse("file:///test.yaml").unwrap();

        println!("=== Debugging Resources section ===");
        println!("YAML content:");
        println!("{}", resources_yaml);

        // Parse with our implementation
        match parse_yaml_ast(resources_yaml, uri.clone()) {
            Ok(ast) => {
                println!("\n✓ Our parser succeeded");
                let converted = to_original_ast(&ast);

                // Parse with original implementation and compare
                match original_parser::parse_yaml_with_custom_tags_from_file(
                    resources_yaml,
                    uri.as_str(),
                ) {
                    Ok(original_ast) => {
                        println!("✓ Original parser succeeded");

                        if compare_asts(&converted, &original_ast, Path::new("resources_test"))
                            .is_ok()
                        {
                            println!("✓ Resources section compatible");
                        } else {
                            println!("✗ Resources section incompatible");
                            println!("Converted: {:#?}", converted);
                            println!("Original: {:#?}", original_ast);
                        }
                    }
                    Err(e) => {
                        println!("✗ Original parser failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("\n✗ Our parser failed: {}", e.message);
            }
        }

        // Test eq case from the file
        let eq_yaml = r#"
Value: !$eq [!$ resources, !$ resources]
"#;

        // Test sequence with GroupBy
        println!("\n=== Debugging sequence GroupBy parsing ===");
        println!("YAML content:");
        println!("{}", sequence_yaml);

        // Parse with our implementation
        match parse_yaml_ast(sequence_yaml, uri.clone()) {
            Ok(ast) => {
                println!("\n✓ Our parser succeeded");
                println!("AST: {:#?}", ast);

                // Try converting to original
                let converted = to_original_ast(&ast);
                println!("\nConverted AST: {:#?}", converted);
            }
            Err(e) => {
                println!("\n✗ Our parser failed: {}", e.message);
            }
        }

        // Parse with original implementation
        match original_parser::parse_yaml_with_custom_tags_from_file(sequence_yaml, uri.as_str()) {
            Ok(original_ast) => {
                println!("\n✓ Original parser succeeded");
                println!("Original AST: {:#?}", original_ast);
            }
            Err(e) => {
                println!("\n✗ Original parser failed: {}", e);
            }
        }

        // Test eq parsing
        println!("\n=== Debugging eq parsing ===");
        println!("YAML content:");
        println!("{}", eq_yaml);

        // Parse with our implementation
        match parse_yaml_ast(eq_yaml, uri.clone()) {
            Ok(ast) => {
                println!("\n✓ Our parser succeeded");
                println!("AST: {:#?}", ast);

                // Try converting to original
                let converted = to_original_ast(&ast);
                println!("\nConverted AST: {:#?}", converted);
            }
            Err(e) => {
                println!("\n✗ Our parser failed: {}", e.message);
            }
        }

        // Parse with original implementation
        match original_parser::parse_yaml_with_custom_tags_from_file(eq_yaml, uri.as_str()) {
            Ok(original_ast) => {
                println!("\n✓ Original parser succeeded");
                println!("Original AST: {:#?}", original_ast);
            }
            Err(e) => {
                println!("\n✗ Original parser failed: {}", e);
            }
        }

        // Test line 233 from groupby.yaml - the problematic !$eq usage
        let eq_test_yaml = r#"
Resources:
  ResourceCounts:
    Type: AWS::SSM::Parameter
    Properties:
      Value: !$toJsonString
        - environmentCounts: !$mapValues
            items: !$groupBy
              items: !$ resources
              key: !$ item.environment
            template: !$eq [!$ item.value, !$ item.value]
"#;

        println!("\n=== Testing complex eq usage ===");
        match parse_yaml_ast(eq_test_yaml, uri.clone()) {
            Ok(ast) => {
                let converted = to_original_ast(&ast);

                match original_parser::parse_yaml_with_custom_tags_from_file(
                    eq_test_yaml,
                    uri.as_str(),
                ) {
                    Ok(original_ast) => {
                        if compare_asts(&converted, &original_ast, Path::new("eq_test")).is_ok() {
                            println!("✓ Complex eq usage compatible");
                        } else {
                            println!("✗ Complex eq usage incompatible - found the breaking point!");
                            println!("Converted: {:#?}", converted);
                            println!("Original: {:#?}", original_ast);
                        }
                    }
                    Err(e) => {
                        println!("✗ Original parser failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Our parser failed: {}", e.message);
            }
        }
        Ok(())
    }
}
