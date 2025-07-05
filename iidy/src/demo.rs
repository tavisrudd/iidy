use anyhow::{Context, Result};
use crossterm::{style::{Stylize, Color}, terminal::size};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};

use iidy::yaml::preprocess_yaml_v11;

#[derive(Debug, Clone)]
enum DemoCommand {
    Shell(String),
    Silent(String),
    Sleep(u64),
    SetEnv(HashMap<String, String>),
    Banner(String),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawCommand {
    // Order matters for serde(untagged) - more specific variants first
    Silent { silent: String },
    Sleep { sleep: u64 },
    SetEnv { setenv: HashMap<String, String> },
    Banner { banner: String },
    // String variant last to catch bare strings as shell commands
    Shell(String),
}

#[derive(Debug, Deserialize)]
struct DemoScript {
    #[serde(default)]
    files: HashMap<String, String>,
    demo: Vec<RawCommand>,
}

pub async fn run(script_path: &str, timescaling: f64) -> Result<()> {
    let data = fs::read_to_string(script_path).with_context(|| format!("reading {script_path}"))?;
    
    // Preprocess YAML with imports and template variables
    let processed_yaml = preprocess_yaml_v11(&data, script_path).await
        .with_context(|| "preprocessing demo script YAML")?;
    
    let script: DemoScript = serde_yaml::from_value(processed_yaml)
        .with_context(|| "parsing preprocessed demo script")?;

    let tmp = tempdir()?;
    unpack_files(&script.files, tmp.path())?;

    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.insert("PKG_SKIP_EXECPATH_PATCH".into(), "yes".into());
    
    // Add current executable path for demo scripts to reference
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_path) = current_exe.to_str() {
            env.insert("IIDY_EXE".into(), exe_path.to_string());
        }
    }

    // Get current executable path for command substitution
    let current_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()));
    
    // Check if 'iidy' on PATH points to the same executable
    let should_substitute = if let Some(ref exe_path) = current_exe {
        !is_iidy_on_path_same_as_current_exe(exe_path)
    } else {
        false
    };
    
    let iidy_exe = if should_substitute {
        current_exe.unwrap_or_else(|| "iidy".to_string())
    } else {
        "iidy".to_string()
    };

    let normalized_commands = script.demo.into_iter()
        .map(normalize_raw_command)
        .collect::<Vec<_>>();

    for command in normalized_commands {
        match command {
            DemoCommand::Shell(cmd) => {
                let substituted_cmd = substitute_iidy_command(&cmd, &iidy_exe);
                print_command(&substituted_cmd, timescaling).await?;
                exec(&substituted_cmd, tmp.path(), &env)?;
            }
            DemoCommand::Silent(cmd) => {
                let substituted_cmd = substitute_iidy_command(&cmd, &iidy_exe);
                exec(&substituted_cmd, tmp.path(), &env)?;
            }
            DemoCommand::Sleep(secs) => {
                let scaled_duration = (secs as f64 * timescaling) as u64;
                sleep(Duration::from_secs(scaled_duration)).await;
            }
            DemoCommand::SetEnv(setenv) => {
                env.extend(setenv);
            }
            DemoCommand::Banner(banner) => {
                display_banner(&banner);
            }
        }
    }

    Ok(())
}

fn normalize_raw_command(raw: RawCommand) -> DemoCommand {
    match raw {
        RawCommand::Shell(cmd) => DemoCommand::Shell(cmd),
        RawCommand::Silent { silent } => DemoCommand::Silent(silent),
        RawCommand::Sleep { sleep } => DemoCommand::Sleep(sleep),
        RawCommand::SetEnv { setenv } => DemoCommand::SetEnv(setenv),
        RawCommand::Banner { banner } => DemoCommand::Banner(banner),
    }
}

fn substitute_iidy_command(cmd: &str, iidy_exe: &str) -> String {
    // Replace 'iidy' at word boundaries with the current executable path
    // This handles cases like:
    // - "iidy help" -> "/path/to/iidy help"
    // - "iidy create-stack file.yaml" -> "/path/to/iidy create-stack file.yaml"
    // - "some-other-command | iidy render" -> "some-other-command | /path/to/iidy render"
    
    // Use regex-like pattern matching for word boundaries
    let mut result = String::new();
    let mut i = 0;
    
    while i < cmd.len() {
        if cmd[i..].starts_with("iidy") {
            // Check if this is a word boundary (start of string or preceded by whitespace/special chars)
            let is_word_start = i == 0 || 
                cmd.chars().nth(i - 1).map_or(false, |c| !c.is_alphanumeric() && c != '_');
            
            // Check if this is followed by a word boundary (end of string or followed by whitespace/special chars)
            let is_word_end = i + 4 >= cmd.len() ||
                cmd.chars().nth(i + 4).map_or(false, |c| !c.is_alphanumeric() && c != '_');
            
            if is_word_start && is_word_end {
                result.push_str(iidy_exe);
                i += 4; // Skip "iidy"
            } else {
                result.push(cmd.chars().nth(i).unwrap());
                i += 1;
            }
        } else {
            result.push(cmd.chars().nth(i).unwrap());
            i += 1;
        }
    }
    
    result
}

fn is_iidy_on_path_same_as_current_exe(current_exe_path: &str) -> bool {
    // Try to find 'iidy' executable on PATH using 'which' command
    let which_output = Command::new("which")
        .arg("iidy")
        .output()
        .ok();
    
    if let Some(output) = which_output {
        if output.status.success() {
            let iidy_path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !iidy_path_str.is_empty() {
                // Convert both paths to canonical form for comparison
                let current_canonical = PathBuf::from(current_exe_path)
                    .canonicalize()
                    .ok();
                let iidy_canonical = PathBuf::from(&iidy_path_str)
                    .canonicalize()
                    .ok();
                
                match (current_canonical, iidy_canonical) {
                    (Some(current), Some(iidy)) => current == iidy,
                    _ => false,
                }
            } else {
                false
            }
        } else {
            false
        }
    } else {
        // No 'which' command available or failed to execute
        false
    }
}

fn unpack_files(files: &HashMap<String, String>, tmp_dir: &Path) -> Result<()> {
    for (path, contents) in files.iter() {
        // Security validation: ensure paths are relative and safe
        if Path::new(path).is_absolute() {
            anyhow::bail!("Illegal path {}. Must be relative.", path);
        }
        
        if path.contains("..") {
            anyhow::bail!("Illegal path {}. Cannot contain parent directory references.", path);
        }

        let full = tmp_dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = File::create(&full)?;
        f.write_all(contents.as_bytes())?;
    }
    Ok(())
}

fn exec(cmd: &str, cwd: &Path, env: &HashMap<String, String>) -> Result<()> {
    let status = Command::new("/usr/bin/env")
        .arg("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .envs(env)
        .status()?;
    if !status.success() {
        anyhow::bail!("command failed: {}", cmd);
    }
    Ok(())
}

async fn print_command(cmd: &str, timescaling: f64) -> Result<()> {
    print!("{} ", "Shell Prompt >".red());
    for ch in cmd.chars() {
        print!("{}", ch.to_string().white()); // \x1b[37m
        std::io::stdout().flush().ok();
        let delay_ms = (50.0 * timescaling) as u64;
        sleep(Duration::from_millis(delay_ms)).await;
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_iidy_command() {
        let exe_path = "/path/to/iidy";

        // Test basic substitution
        assert_eq!(
            substitute_iidy_command("iidy help", exe_path),
            "/path/to/iidy help"
        );

        // Test with arguments
        assert_eq!(
            substitute_iidy_command("iidy create-stack file.yaml", exe_path),
            "/path/to/iidy create-stack file.yaml"
        );

        // Test with pipes
        assert_eq!(
            substitute_iidy_command("cat file.yaml | iidy render", exe_path),
            "cat file.yaml | /path/to/iidy render"
        );

        // Test with complex commands
        assert_eq!(
            substitute_iidy_command("iidy list-stacks | grep test", exe_path),
            "/path/to/iidy list-stacks | grep test"
        );

        // Test that it doesn't substitute partial matches
        assert_eq!(
            substitute_iidy_command("myiidy command", exe_path),
            "myiidy command"
        );

        assert_eq!(
            substitute_iidy_command("iidycommand", exe_path),
            "iidycommand"
        );

        // Test multiple occurrences
        assert_eq!(
            substitute_iidy_command("iidy create && iidy update", exe_path),
            "/path/to/iidy create && /path/to/iidy update"
        );

        // Test with quotes
        assert_eq!(
            substitute_iidy_command("echo 'running iidy'", exe_path),
            "echo 'running /path/to/iidy'"
        );
    }

    #[test]
    fn test_is_iidy_on_path_same_as_current_exe() {
        // Test with non-existent path
        assert_eq!(
            is_iidy_on_path_same_as_current_exe("/non/existent/path"),
            false
        );
        
        // Test with current executable if we can determine it
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(current_path) = current_exe.to_str() {
                // This should work regardless of whether iidy is on PATH
                let result = is_iidy_on_path_same_as_current_exe(current_path);
                // We can't assert a specific value since it depends on the environment
                // but we can ensure the function doesn't panic
                let _ = result;
            }
        }
    }
}

fn display_banner(text: &str) {
    let (cols, _) = size().unwrap_or((80, 0));
    let line = " ".repeat(cols as usize);
    
    // Use ANSI 256-color 236 to match iidy-js exactly (cli.bgXterm(236))
    let bg_color = Color::AnsiValue(236);
    
    println!(); // Blank line before banner
    println!("{}", line.clone().on(bg_color));
    for ln in text.split('\n') {
        let padding = if ln.len() + 2 >= cols as usize {
            0
        } else {
            cols as usize - ln.len() - 2
        };
        let msg = format!("  {}{}", ln, " ".repeat(padding));
        println!("{}", msg.yellow().on(bg_color).bold());
    }
    println!("{}", line.on(bg_color));
    println!(); // Blank line after banner
}
