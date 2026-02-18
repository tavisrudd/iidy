//! Snapshot tests for error example templates
//!
//! This test suite captures the enhanced error outputs for various error
//! conditions using the error examples in example-templates/errors/
//!
//! Each error template file gets its own test function to allow insta to
//! collect all snapshot failures in one run.
use std::fs;
use std::path::{Path, PathBuf};

use iidy::cli::YamlSpec;
use iidy::yaml::preprocess_yaml;
use insta::assert_snapshot;

fn discover_templates(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut templates = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if path.is_file()
                && path.extension().map_or(false, |ext| ext == "yaml")
                && !name.starts_with(".")
            {
                templates.push((path, name));
            } else if path.is_dir() {
                // Recursively discover in subdirectories
                templates.extend(discover_templates(&path));
            }
        }
    }
    templates.sort_by(|a, b| a.0.cmp(&b.0)); // Sort for consistent order
    templates
}

// This function is used by the main test to process each error template
async fn test_error_template(path: &Path) {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }

    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", path, e));
    let snapshot_name = format!(
        "auto_discovered_{}",
        path.to_str()
            .unwrap()
            .replace("/", "_")
            .replace("-", "_")
            .replace(".yaml", "")
    );

    match preprocess_yaml(&content, &path.to_str().unwrap(), &YamlSpec::Auto).await {
        Ok(_) => panic!(
            "Expected {} to fail but it succeeded",
            path.to_str().unwrap()
        ),
        Err(e) => {
            // Convert error to string to capture the enhanced error display
            assert_snapshot!(snapshot_name, format!("{}", e))
        }
    }
}

// Alternative approach: Use a single test but with better failure collection
#[tokio::test]
async fn test_all_example_errors_auto_discovery() {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }

    let example_dir = Path::new("example-templates/errors/");
    let discovered_templates = discover_templates(example_dir);

    // Collect all errors instead of panicking on first failure
    let mut failures = Vec::new();

    for (path, _filename) in discovered_templates {
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                failures.push(format!("Failed to read {:?}: {}", path, e));
                continue;
            }
        };

        let snapshot_name = format!(
            "auto_discovered_{}",
            path.to_str()
                .unwrap()
                .replace("/", "_")
                .replace("-", "_")
                .replace(".yaml", "")
        );

        match preprocess_yaml(&content, &path.to_str().unwrap(), &YamlSpec::Auto).await {
            Ok(_) => {
                failures.push(format!(
                    "Expected {} to fail but it succeeded",
                    path.to_str().unwrap()
                ));
            }
            Err(e) => {
                // Use our helper function with error catching for better test failure collection
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // Can't use async in catch_unwind, so inline the logic
                    assert_snapshot!(snapshot_name, format!("{}", e));
                })) {
                    Ok(_) => {
                        // Snapshot matched or was accepted
                    }
                    Err(_) => {
                        // Snapshot failed - insta should have created .new files
                        failures.push(format!(
                            "Snapshot mismatch for {} (check .snap.new files)",
                            path.display()
                        ));
                    }
                }
            }
        }
    }

    // Report all failures at the end
    if !failures.is_empty() {
        panic!(
            "Found {} test failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

// Test to verify the helper function works properly
#[tokio::test]
async fn test_error_template_helper_function() {
    // Test that our helper function works with a known error file
    let error_file = Path::new("example-templates/errors/variable-not-found.yaml");
    if error_file.exists() {
        // This should create a snapshot or pass if already exists
        test_error_template(error_file).await;
    }
}
