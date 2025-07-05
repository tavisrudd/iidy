use anyhow::{Context, Result};
use crossterm::{style::Stylize, terminal::size};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
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

pub async fn run(script_path: &str, timescaling: u32) -> Result<()> {
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

    let normalized_commands = script.demo.into_iter()
        .map(normalize_raw_command)
        .collect::<Vec<_>>();

    for command in normalized_commands {
        match command {
            DemoCommand::Shell(cmd) => {
                print_command(&cmd, timescaling).await?;
                exec(&cmd, tmp.path(), &env)?;
            }
            DemoCommand::Silent(cmd) => {
                exec(&cmd, tmp.path(), &env)?;
            }
            DemoCommand::Sleep(secs) => {
                sleep(Duration::from_secs(secs * timescaling as u64)).await;
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

async fn print_command(cmd: &str, timescaling: u32) -> Result<()> {
    print!("{} ", "Shell Prompt >".red());
    for ch in cmd.chars() {
        print!("{}", ch.to_string().white()); // \x1b[37m
        std::io::stdout().flush().ok();
        sleep(Duration::from_millis(50 * timescaling as u64)).await;
    }
    println!();
    Ok(())
}

fn display_banner(text: &str) {
    let (cols, _) = size().unwrap_or((80, 0));
    let line = " ".repeat(cols as usize);
    println!("{}", line.clone().grey().on_dark_grey());
    for ln in text.split('\n') {
        let padding = cols as usize - ln.len() - 2;
        let msg = format!("  {}{}", ln, " ".repeat(padding.max(0)));
        println!("{}", msg.yellow().on_dark_grey().bold());
    }
    println!("{}", line.grey().on_dark_grey());
    println!();
}
