//! Snapshot tests for error example templates
//! 
//! This test suite captures the enhanced error outputs for various error 
//! conditions using the error examples in example-templates/errors/
use std::fs;
use std::path::{Path, PathBuf};

use iidy::yaml::preprocess_yaml;
use iidy::cli::YamlSpec;
use insta::assert_snapshot;

#[tokio::test]
async fn test_all_example_errors_auto_discovery() {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    fn discover_templates(dir: &Path) -> Vec<(PathBuf, String)> {
        let mut templates = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                
                if path.is_file() && 
                   path.extension().map_or(false, |ext| ext == "yaml") &&
                   !name.starts_with(".") {
                    templates.push((path, name));
                } else if path.is_dir() {
                    // Recursively discover in subdirectories
                    templates.extend(discover_templates(&path));
                }
            }
        }
        templates
    }
    
    let example_dir = Path::new("example-templates/errors/");
    let discovered_templates = discover_templates(example_dir);
    for (path, _filename) in discovered_templates {
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", path, e));
        let snapshot_name = format!(
            "auto_discovered_{}", 
            path.to_str().unwrap().replace("/", "_").replace("-", "_").replace(".yaml", ""));

            match preprocess_yaml(&content, &path.to_str().unwrap(), &YamlSpec::Auto).await {
                Ok(_) => panic!("Expected {} to fail but it succeeded", path.to_str().unwrap()),
                Err(e) => {
                    // Convert error to string to capture the enhanced error display
                    assert_snapshot!(snapshot_name, format!("{}", e))
            }
        }
    }
}
