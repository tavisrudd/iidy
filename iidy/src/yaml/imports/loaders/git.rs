//! Git command execution import loader
//! 
//! Provides functionality for executing git commands and retrieving git information

use anyhow::{Result, anyhow};
use serde_json::Value;
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