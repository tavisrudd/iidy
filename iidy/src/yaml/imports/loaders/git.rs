//! Git command execution import loader
//! 
//! Provides functionality for executing git commands and retrieving git information

use anyhow::{Result, anyhow};
use serde_yaml::Value;
use async_trait::async_trait;

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

/// Execute a git command and return the output
async fn execute_git_command(command: &str) -> Result<String> {
    let executor = SystemGitExecutor;
    executor.execute(command).await
}

/// Load a git import (branch, describe, sha)
pub async fn load_git_import(location: &str, _base_location: &str) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0] != "git" {
        return Err(anyhow!("Invalid git import format: {}", location));
    }

    let git_command = parts[1];
    let data = match git_command {
        "branch" => execute_git_command("git rev-parse --abbrev-ref HEAD").await?,
        "describe" => execute_git_command("git describe --dirty --tags").await?,
        "sha" => execute_git_command("git rev-parse HEAD").await?,
        _ => return Err(anyhow!("Invalid git command: {}", location)),
    };

    Ok(ImportData {
        import_type: ImportType::Git,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Load a git import with custom executor (for testing)
pub async fn load_git_import_with_executor(
    location: &str, 
    _base_location: &str, 
    executor: &dyn GitCommandExecutor
) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0] != "git" {
        return Err(anyhow!("Invalid git import format: {}", location));
    }

    let git_command_type = parts[1];
    let command = match git_command_type {
        "branch" => "git rev-parse --abbrev-ref HEAD",
        "describe" => "git describe --dirty --tags",
        "sha" => "git rev-parse HEAD",
        _ => return Err(anyhow!("Invalid git command: {}", location)),
    };
    
    let data = executor.execute(command).await?;

    Ok(ImportData {
        import_type: ImportType::Git,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
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
        let executor = MockGitExecutor::new()
            .expect_command("git describe --dirty --tags", Ok("v1.2.3-4-gabcdef".to_string()));

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
        let executor = MockGitExecutor::new()
            .expect_command("git rev-parse HEAD", Ok(sha.to_string()));

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
        assert!(result.unwrap_err().to_string().contains("Invalid git command"));
    }

    #[tokio::test]
    async fn test_load_git_import_invalid_format() {
        let executor = MockGitExecutor::new();
        let result = load_git_import_with_executor("invalid:format", "/base", &executor).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid git import format"));
    }

    #[tokio::test]
    async fn test_load_git_import_command_failure() {
        let executor = MockGitExecutor::new()
            .expect_command("git rev-parse --abbrev-ref HEAD", Err(anyhow!("Git command failed")));

        let result = load_git_import_with_executor("git:branch", "/base", &executor).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Git command failed"));
    }

    #[tokio::test]
    async fn test_load_git_import_dirty_describe() -> Result<()> {
        let executor = MockGitExecutor::new()
            .expect_command("git describe --dirty --tags", Ok("v1.0.0-dirty".to_string()));

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

    // Note: We don't test the direct load_git_import function as it would require
    // a real git repository and git command to be available. In a real test environment,
    // you might want to create integration tests that verify the SystemGitExecutor
    // works correctly in a controlled git repository.
    //
    // Fun fact: Testing git commands is like trying to herd cats - just when you think
    // you've got them all accounted for, one runs off and creates a detached HEAD! 🐱‍💻
}