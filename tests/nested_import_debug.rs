//! Debug trace tests for nested import environment handling
//!
//! These tests are designed to validate our understanding of how the current
//! implementation handles nested document environments when importing.
//! Tests go up to 3 levels deep to understand the behavior thoroughly.

use anyhow::Result;
use iidy::yaml::preprocess_yaml_v11;
use serde_yaml::Value;
use std::io::Write;
use tempfile::NamedTempFile;

/// Test 1-level deep import: Simple import with $defs
#[tokio::test]
async fn test_debug_1_level_import() -> Result<()> {
    println!("\n=== DEBUG: 1-Level Import Test ===");

    // Create Level 1 (imported) document
    let mut level1_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(level1_file, "$defs:")?;
    writeln!(level1_file, "  level1_var: \"L1_VALUE\"")?;
    writeln!(level1_file, "  shared_var: \"L1_SHARED\"")?;
    writeln!(level1_file, "")?;
    writeln!(level1_file, "config:")?;
    writeln!(
        level1_file,
        "  processed_value: \"{{level1_var}}-processed\""
    )?;
    writeln!(level1_file, "  raw_value: \"raw-data\"")?;
    writeln!(level1_file, "  shared_check: \"{{shared_var}}\"")?;
    let level1_path = level1_file.path().to_string_lossy().to_string();

    println!("Level 1 document path: {}", level1_path);
    println!("Level 1 content:");
    println!("$defs:");
    println!("  level1_var: \"L1_VALUE\"");
    println!("  shared_var: \"L1_SHARED\"");
    println!("config:");
    println!("  processed_value: \"{{{{level1_var}}}}-processed\"");
    println!("  raw_value: \"raw-data\"");
    println!("  shared_check: \"{{{{shared_var}}}}\"");

    // Create Level 0 (main) document
    let main_yaml = format!(
        r#"
$defs:
  main_var: "MAIN_VALUE"
  shared_var: "MAIN_SHARED"

$imports:
  imported: "{}"

# Test accessing imported data
result_raw: !$ imported.config.raw_value
result_processed: !$ imported.config.processed_value
result_shared: !$ imported.config.shared_check

# Test main document variables
main_check: "{{{{main_var}}}}"
main_shared_check: "{{{{shared_var}}}}"
"#,
        level1_path
    );

    println!("\nMain document:");
    println!("{}", main_yaml);

    let result = preprocess_yaml_v11(&main_yaml, "main.yaml").await?;

    println!("\n=== RESULT ===");
    let result_yaml = serde_yaml::to_string(&result)?;
    println!("{}", result_yaml);

    // Debug: Print the structure
    if let Value::Mapping(map) = &result {
        println!("\n=== DETAILED ANALYSIS ===");

        println!("Top-level keys present:");
        for key in map.keys() {
            println!("  - {}", serde_yaml::to_string(key)?.trim());
        }

        println!("\nPreprocessing directives check:");
        println!(
            "  $defs present: {}",
            map.contains_key(&Value::String("$defs".to_string()))
        );
        println!(
            "  $imports present: {}",
            map.contains_key(&Value::String("$imports".to_string()))
        );

        println!("\nMain document variables:");
        if let Some(main_check) = map.get(&Value::String("main_check".to_string())) {
            println!("  main_check: {:?}", main_check);
        }
        if let Some(main_shared) = map.get(&Value::String("main_shared_check".to_string())) {
            println!("  main_shared_check: {:?}", main_shared);
        }

        println!("\nImported data access:");
        if let Some(result_raw) = map.get(&Value::String("result_raw".to_string())) {
            println!("  result_raw: {:?}", result_raw);
        }
        if let Some(result_processed) = map.get(&Value::String("result_processed".to_string())) {
            println!("  result_processed: {:?}", result_processed);
        }
        if let Some(result_shared) = map.get(&Value::String("result_shared".to_string())) {
            println!("  result_shared: {:?}", result_shared);
        }
    }

    Ok(())
}

/// Test 2-level deep import: Import that imports another document
#[tokio::test]
async fn test_debug_2_level_import() -> Result<()> {
    println!("\n=== DEBUG: 2-Level Import Test ===");

    // Create Level 2 (deepest) document
    let mut level2_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(level2_file, "$defs:")?;
    writeln!(level2_file, "  level2_var: \"L2_VALUE\"")?;
    writeln!(level2_file, "  shared_var: \"L2_SHARED\"")?;
    writeln!(level2_file, "")?;
    writeln!(level2_file, "deep_config:")?;
    writeln!(level2_file, "  deep_value: \"{{level2_var}}-deep\"")?;
    writeln!(level2_file, "  deep_shared: \"{{shared_var}}\"")?;
    let level2_path = level2_file.path().to_string_lossy().to_string();

    // Create Level 1 (middle) document that imports Level 2
    let mut level1_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(level1_file, "$defs:")?;
    writeln!(level1_file, "  level1_var: \"L1_VALUE\"")?;
    writeln!(level1_file, "  shared_var: \"L1_SHARED\"")?;
    writeln!(level1_file, "")?;
    writeln!(level1_file, "$imports:")?;
    writeln!(level1_file, "  level2_data: \"{}\"", level2_path)?;
    writeln!(level1_file, "")?;
    writeln!(level1_file, "config:")?;
    writeln!(
        level1_file,
        "  level1_processed: \"{{level1_var}}-processed\""
    )?;
    writeln!(level1_file, "  level1_shared: \"{{shared_var}}\"")?;
    writeln!(level1_file, "  # Accessing level2 data")?;
    writeln!(
        level1_file,
        "  deep_access: !$ level2_data.deep_config.deep_value"
    )?;
    let level1_path = level1_file.path().to_string_lossy().to_string();

    println!("Level 2 (deepest) path: {}", level2_path);
    println!("Level 1 (middle) path: {}", level1_path);

    // Create Level 0 (main) document
    let main_yaml = format!(
        r#"
$defs:
  main_var: "MAIN_VALUE"
  shared_var: "MAIN_SHARED"

$imports:
  imported: "{}"

# Test accessing nested imported data
result_l1: !$ imported.config.level1_processed
result_l1_shared: !$ imported.config.level1_shared
result_l2_via_l1: !$ imported.config.deep_access

# Test main document variables
main_check: "{{{{main_var}}}}"
main_shared_check: "{{{{shared_var}}}}"
"#,
        level1_path
    );

    println!("\nDocument hierarchy:");
    println!("  Main -> Level1 -> Level2");
    println!("\nMain document imports: Level1");
    println!("Level1 document imports: Level2");

    let result = preprocess_yaml_v11(&main_yaml, "main.yaml").await?;

    println!("\n=== RESULT ===");
    let result_yaml = serde_yaml::to_string(&result)?;
    println!("{}", result_yaml);

    if let Value::Mapping(map) = &result {
        println!("\n=== ENVIRONMENT ISOLATION ANALYSIS ===");

        println!("Main document variable processing:");
        if let Some(main_check) = map.get(&Value::String("main_check".to_string())) {
            println!("  main_check: {:?} (should be 'MAIN_VALUE')", main_check);
        }
        if let Some(main_shared) = map.get(&Value::String("main_shared_check".to_string())) {
            println!(
                "  main_shared_check: {:?} (should be 'MAIN_SHARED')",
                main_shared
            );
        }

        println!("\nLevel 1 data access:");
        if let Some(result_l1) = map.get(&Value::String("result_l1".to_string())) {
            println!(
                "  result_l1: {:?} (should show L1_VALUE processing)",
                result_l1
            );
        }
        if let Some(result_l1_shared) = map.get(&Value::String("result_l1_shared".to_string())) {
            println!(
                "  result_l1_shared: {:?} (should show L1_SHARED)",
                result_l1_shared
            );
        }

        println!("\nLevel 2 data access (via Level 1):");
        if let Some(result_l2) = map.get(&Value::String("result_l2_via_l1".to_string())) {
            println!(
                "  result_l2_via_l1: {:?} (should show L2_VALUE processing)",
                result_l2
            );
        }
    }

    Ok(())
}

/// Test 3-level deep import: Maximum nesting to understand environment behavior
#[tokio::test]
async fn test_debug_3_level_import() -> Result<()> {
    println!("\n=== DEBUG: 3-Level Import Test ===");

    // Create Level 3 (deepest) document
    let mut level3_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(level3_file, "$defs:")?;
    writeln!(level3_file, "  level3_var: \"L3_VALUE\"")?;
    writeln!(level3_file, "  shared_var: \"L3_SHARED\"")?;
    writeln!(level3_file, "")?;
    writeln!(level3_file, "deepest:")?;
    writeln!(level3_file, "  value: \"{{level3_var}}-deepest\"")?;
    writeln!(level3_file, "  shared_check: \"{{shared_var}}\"")?;
    let level3_path = level3_file.path().to_string_lossy().to_string();

    // Create Level 2 (middle-deep) document
    let mut level2_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(level2_file, "$defs:")?;
    writeln!(level2_file, "  level2_var: \"L2_VALUE\"")?;
    writeln!(level2_file, "  shared_var: \"L2_SHARED\"")?;
    writeln!(level2_file, "")?;
    writeln!(level2_file, "$imports:")?;
    writeln!(level2_file, "  level3_data: \"{}\"", level3_path)?;
    writeln!(level2_file, "")?;
    writeln!(level2_file, "deep_config:")?;
    writeln!(level2_file, "  level2_value: \"{{level2_var}}-deep\"")?;
    writeln!(level2_file, "  level2_shared: \"{{shared_var}}\"")?;
    writeln!(level2_file, "  level3_access: !$ level3_data.deepest.value")?;
    let level2_path = level2_file.path().to_string_lossy().to_string();

    // Create Level 1 (middle) document
    let mut level1_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(level1_file, "$defs:")?;
    writeln!(level1_file, "  level1_var: \"L1_VALUE\"")?;
    writeln!(level1_file, "  shared_var: \"L1_SHARED\"")?;
    writeln!(level1_file, "")?;
    writeln!(level1_file, "$imports:")?;
    writeln!(level1_file, "  level2_data: \"{}\"", level2_path)?;
    writeln!(level1_file, "")?;
    writeln!(level1_file, "config:")?;
    writeln!(level1_file, "  level1_value: \"{{level1_var}}-processed\"")?;
    writeln!(level1_file, "  level1_shared: \"{{shared_var}}\"")?;
    writeln!(
        level1_file,
        "  level2_access: !$ level2_data.deep_config.level2_value"
    )?;
    writeln!(
        level1_file,
        "  level3_via_level2: !$ level2_data.deep_config.level3_access"
    )?;
    let level1_path = level1_file.path().to_string_lossy().to_string();

    println!("Document hierarchy:");
    println!("  Main -> Level1 -> Level2 -> Level3");
    println!("  Paths:");
    println!("    Level3: {}", level3_path);
    println!("    Level2: {}", level2_path);
    println!("    Level1: {}", level1_path);

    // Create Level 0 (main) document
    let main_yaml = format!(
        r#"
$defs:
  main_var: "MAIN_VALUE"
  shared_var: "MAIN_SHARED"

$imports:
  imported: "{}"

# Test accessing data at all levels
result_l1: !$ imported.config.level1_value
result_l1_shared: !$ imported.config.level1_shared
result_l2_via_l1: !$ imported.config.level2_access
result_l3_via_l2_via_l1: !$ imported.config.level3_via_level2

# Test main environment isolation
main_check: "{{{{main_var}}}}"
main_shared_check: "{{{{shared_var}}}}"
"#,
        level1_path
    );

    let result = preprocess_yaml_v11(&main_yaml, "main.yaml").await?;

    println!("\n=== RESULT ===");
    let result_yaml = serde_yaml::to_string(&result)?;
    println!("{}", result_yaml);

    if let Value::Mapping(map) = &result {
        println!("\n=== DEEP NESTING ANALYSIS ===");

        println!("Environment isolation check:");
        if let Some(main_check) = map.get(&Value::String("main_check".to_string())) {
            println!("  Main environment works: {:?}", main_check);
        }
        if let Some(main_shared) = map.get(&Value::String("main_shared_check".to_string())) {
            println!("  Main shared_var precedence: {:?}", main_shared);
        }

        println!("\nNested data access chain:");
        if let Some(result_l1) = map.get(&Value::String("result_l1".to_string())) {
            println!("  Level 1 data: {:?}", result_l1);
        }
        if let Some(result_l2) = map.get(&Value::String("result_l2_via_l1".to_string())) {
            println!("  Level 2 via Level 1: {:?}", result_l2);
        }
        if let Some(result_l3) = map.get(&Value::String("result_l3_via_l2_via_l1".to_string())) {
            println!("  Level 3 via Level 2 via Level 1: {:?}", result_l3);
        }

        println!("\nShared variable behavior at each level:");
        if let Some(result_l1_shared) = map.get(&Value::String("result_l1_shared".to_string())) {
            println!("  Level 1 shared_var resolution: {:?}", result_l1_shared);
        }
    }

    Ok(())
}

/// Test environment variable collision scenarios
#[tokio::test]
async fn test_debug_environment_collisions() -> Result<()> {
    println!("\n=== DEBUG: Environment Collision Test ===");

    // Create imported document with conflicting variable names
    let mut imported_file = NamedTempFile::with_suffix(".yaml")?;
    writeln!(imported_file, "$defs:")?;
    writeln!(imported_file, "  conflict_var: \"IMPORTED_VALUE\"")?;
    writeln!(imported_file, "  imported_only: \"ONLY_IN_IMPORTED\"")?;
    writeln!(imported_file, "")?;
    writeln!(imported_file, "data:")?;
    writeln!(imported_file, "  result: \"{{conflict_var}}\"")?;
    writeln!(imported_file, "  unique: \"{{imported_only}}\"")?;
    let imported_path = imported_file.path().to_string_lossy().to_string();

    // Create main document with conflicting variable
    let main_yaml = format!(
        r#"
$defs:
  conflict_var: "MAIN_VALUE"
  main_only: "ONLY_IN_MAIN"

$imports:
  imported_data: "{}"

# Test which environment takes precedence
main_conflict_test: "{{{{conflict_var}}}}"
main_unique_test: "{{{{main_only}}}}"

# Test accessing imported data (should use imported environment)
imported_access: !$ imported_data.data.result
imported_unique_access: !$ imported_data.data.unique

# Test if imported variables leak into main environment
# This should fail if environment isolation works correctly
# leaked_test: "{{imported_only}}"  # Commented out as it should cause error
"#,
        imported_path
    );

    println!("Testing environment collision scenario:");
    println!("  Both main and imported docs define 'conflict_var'");
    println!("  Main: conflict_var = 'MAIN_VALUE'");
    println!("  Imported: conflict_var = 'IMPORTED_VALUE'");

    let result = preprocess_yaml_v11(&main_yaml, "main.yaml").await?;

    println!("\n=== RESULT ===");
    let result_yaml = serde_yaml::to_string(&result)?;
    println!("{}", result_yaml);

    if let Value::Mapping(map) = &result {
        println!("\n=== COLLISION ANALYSIS ===");

        if let Some(main_conflict) = map.get(&Value::String("main_conflict_test".to_string())) {
            println!("  Main environment conflict_var: {:?}", main_conflict);
            println!("    Expected: 'MAIN_VALUE' (main should win)");
        }

        if let Some(imported_access) = map.get(&Value::String("imported_access".to_string())) {
            println!("  Imported data conflict_var: {:?}", imported_access);
            println!("    Expected: 'IMPORTED_VALUE' (imported should use its own)");
        }

        if let Some(main_unique) = map.get(&Value::String("main_unique_test".to_string())) {
            println!("  Main unique variable: {:?}", main_unique);
        }

        if let Some(imported_unique) = map.get(&Value::String("imported_unique_access".to_string()))
        {
            println!("  Imported unique variable: {:?}", imported_unique);
        }
    }

    Ok(())
}
