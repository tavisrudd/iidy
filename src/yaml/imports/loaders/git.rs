//! Git command execution import loader
//!
//! Provides functionality for executing git commands and retrieving git information

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_yaml::Value;

use crate::yaml::imports::{ImportData, ImportType};

/// Trait for executing git commands (allows mocking in tests)
#[async_trait]
pub trait GitCommandExecutor: Send + Sync {
    async fn execute(&self, command: &str) -> Result<String>;
}

/// Production git command executor
pub struct SystemGitExecutor;

#[async_trait]
impl GitCommandExecutor for SystemGitExecutor {
    async fn execute(&self, command: &str) -> Result<String> {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow!("Git command failed: {}", command));
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
}

/// Map git command type to actual git command string
fn get_git_command(git_command_type: &str) -> Result<&'static str> {
    match git_command_type {
        "branch" => Ok("git rev-parse --abbrev-ref HEAD"),
        "describe" => Ok("git describe --always --dirty --tags"),
        "sha" => Ok("git rev-parse HEAD"),
        _ => Err(anyhow!("Invalid git command: {}", git_command_type)),
    }
}

/// Parse and validate git import location format
fn parse_git_location(location: &str) -> Result<&str> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0] != "git" {
        return Err(anyhow!("Invalid git import format: {}", location));
    }
    Ok(parts[1])
}

/// Create ImportData from git command output
fn create_git_import_data(location: &str, data: String) -> ImportData {
    ImportData {
        import_type: ImportType::Git,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    }
}

/// Load a git import (branch, describe, sha)
pub async fn load_git_import(location: &str, base_location: &str) -> Result<ImportData> {
    let executor = SystemGitExecutor;
    load_git_import_with_executor(location, base_location, &executor).await
}

/// Load a git import with custom executor (for testing)
pub async fn load_git_import_with_executor(
    location: &str,
    _base_location: &str,
    executor: &dyn GitCommandExecutor,
) -> Result<ImportData> {
    let git_command_type = parse_git_location(location)?;
    let command = get_git_command(git_command_type)?;
    let data = executor.execute(command).await?;
    Ok(create_git_import_data(location, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock git executor for testing
    struct MockGitExecutor {
        responses: std::collections::HashMap<String, Result<String>>,
    }

    impl MockGitExecutor {
        fn new() -> Self {
            Self {
                responses: std::collections::HashMap::new(),
            }
        }

        fn expect_command(mut self, command: &str, response: Result<String>) -> Self {
            self.responses.insert(command.to_string(), response);
            self
        }
    }

    #[async_trait]
    impl GitCommandExecutor for MockGitExecutor {
        async fn execute(&self, command: &str) -> Result<String> {
            match self.responses.get(command) {
                Some(Ok(output)) => Ok(output.clone()),
                Some(Err(e)) => Err(anyhow!("{}", e)),
                None => Err(anyhow!("Unexpected command: {}", command)),
            }
        }
    }

    #[tokio::test]
    async fn test_load_git_import_branch() -> Result<()> {
        let executor = MockGitExecutor::new()
            .expect_command("git rev-parse --abbrev-ref HEAD", Ok("main".to_string()));

        let result = load_git_import_with_executor("git:branch", "/base", &executor).await?;

        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.resolved_location, "git:branch");
        assert_eq!(result.data, "main");
        assert_eq!(result.doc, Value::String("main".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_git_import_describe() -> Result<()> {
        let executor = MockGitExecutor::new().expect_command(
            "git describe --always --dirty --tags",
            Ok("v1.2.3-4-gabcdef".to_string()),
        );

        let result = load_git_import_with_executor("git:describe", "/base", &executor).await?;

        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.resolved_location, "git:describe");
        assert_eq!(result.data, "v1.2.3-4-gabcdef");
        assert_eq!(result.doc, Value::String("v1.2.3-4-gabcdef".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_git_import_sha() -> Result<()> {
        let sha = "abcdef1234567890abcdef1234567890abcdef12";
        let executor =
            MockGitExecutor::new().expect_command("git rev-parse HEAD", Ok(sha.to_string()));

        let result = load_git_import_with_executor("git:sha", "/base", &executor).await?;

        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.resolved_location, "git:sha");
        assert_eq!(result.data, sha);
        assert_eq!(result.doc, Value::String(sha.to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_git_import_invalid_command() {
        let executor = MockGitExecutor::new();
        let result = load_git_import_with_executor("git:invalid", "/base", &executor).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid git command")
        );
    }

    #[tokio::test]
    async fn test_load_git_import_invalid_format() {
        let executor = MockGitExecutor::new();
        let result = load_git_import_with_executor("invalid:format", "/base", &executor).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid git import format")
        );
    }

    #[tokio::test]
    async fn test_load_git_import_command_failure() {
        let executor = MockGitExecutor::new().expect_command(
            "git rev-parse --abbrev-ref HEAD",
            Err(anyhow!("Git command failed")),
        );

        let result = load_git_import_with_executor("git:branch", "/base", &executor).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Git command failed")
        );
    }

    #[tokio::test]
    async fn test_load_git_import_dirty_describe() -> Result<()> {
        let executor = MockGitExecutor::new().expect_command(
            "git describe --always --dirty --tags",
            Ok("v1.0.0-dirty".to_string()),
        );

        let result = load_git_import_with_executor("git:describe", "/base", &executor).await?;

        assert_eq!(result.data, "v1.0.0-dirty");

        Ok(())
    }

    #[tokio::test]
    async fn test_load_git_import_detached_head() -> Result<()> {
        let executor = MockGitExecutor::new()
            .expect_command("git rev-parse --abbrev-ref HEAD", Ok("HEAD".to_string()));

        let result = load_git_import_with_executor("git:branch", "/base", &executor).await?;

        assert_eq!(result.data, "HEAD");

        Ok(())
    }

    #[tokio::test]
    async fn test_load_git_import_describe_no_tags() -> Result<()> {
        // Test --always behavior when no tags exist (falls back to commit hash)
        let executor = MockGitExecutor::new().expect_command(
            "git describe --always --dirty --tags",
            Ok("abcdef1".to_string()),
        );

        let result = load_git_import_with_executor("git:describe", "/base", &executor).await?;

        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.resolved_location, "git:describe");
        assert_eq!(result.data, "abcdef1");
        assert_eq!(result.doc, Value::String("abcdef1".to_string()));

        Ok(())
    }

    // Note: We don't test the direct load_git_import function as it would require
    // a real git repository and git command to be available. In a real test environment,
    // you might want to create integration tests that verify the SystemGitExecutor
    // works correctly in a controlled git repository.
}
