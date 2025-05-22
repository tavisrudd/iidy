use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use anyhow::{Context, Result};
use serde::Deserialize;
use tempfile::tempdir;
use crossterm::{style::{Color, Stylize}, terminal::{self, size}};
use tokio::time::{sleep, Duration};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawCommand {
    Shell(String),
    Silent { silent: String },
    Sleep { sleep: u64 },
    SetEnv { setenv: HashMap<String, String> },
    Banner { banner: String },
}

#[derive(Debug, Deserialize)]
struct DemoScript {
    #[serde(default)]
    files: HashMap<String, String>,
    demo: Vec<RawCommand>,
}

pub async fn run(script_path: &str, timescaling: u32) -> Result<()> {
    let data = fs::read_to_string(script_path)
        .with_context(|| format!("reading {script_path}"))?;
    let script: DemoScript = serde_yaml::from_str(&data)
        .with_context(|| "parsing demo script")?;

    let tmp = tempdir()?;
    for (path, contents) in script.files.iter() {
        let full = tmp.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = File::create(&full)?;
        f.write_all(contents.as_bytes())?;
    }

    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.insert("PKG_SKIP_EXECPATH_PATCH".into(), "yes".into());

    for command in script.demo {
        match command {
            RawCommand::Shell(cmd) => {
                print_command(&cmd).await?;
                exec(&cmd, tmp.path(), &env)?;
            }
            RawCommand::Silent { silent } => {
                exec(&silent, tmp.path(), &env)?;
            }
            RawCommand::Sleep { sleep: secs } => {
                sleep(Duration::from_secs(secs * timescaling as u64)).await;
            }
            RawCommand::SetEnv { setenv } => {
                env.extend(setenv);
            }
            RawCommand::Banner { banner } => {
                display_banner(&banner);
            }
        }
    }

    Ok(())
}

fn exec(cmd: &str, cwd: &Path, env: &HashMap<String, String>) -> Result<()> {
    let status = Command::new("/bin/bash")
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

async fn print_command(cmd: &str) -> Result<()> {
    print!("{} ", "Shell Prompt >".red());
    for ch in cmd.chars() {
        print!("{}", ch.to_string().white());
        std::io::stdout().flush().ok();
        sleep(Duration::from_millis(50)).await;
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
