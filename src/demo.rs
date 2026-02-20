use anyhow::{Context, Result};
use crossterm::{
    style::{Color, Stylize},
    terminal::size,
};
use once_cell::sync::Lazy;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use tempfile::tempdir;
use tokio::time::{Duration, sleep};

use iidy::yaml::preprocess_yaml_v11;

// Compile regexes once at startup for performance
static RE_ACCOUNT_STANDALONE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(\d{12})\b").expect("invalid regex pattern"));

static RE_ACCOUNT_IN_ARN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(arn:aws:[^:]*:[^:]*:)(\d{12})([:/])").expect("invalid regex pattern")
});

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

/// Mask AWS account numbers in text output
///
/// Patterns masked:
/// - Standalone 12-digit account numbers: "account = 123456789012" → "account = ************"
/// - Account numbers in ARNs: "arn:aws:iam::123456789012:role/Foo" → "arn:aws:iam::************:role/Foo"
///
/// Uses pre-compiled regex patterns for performance.
fn mask_aws_account_numbers(text: &str) -> String {
    // Apply standalone pattern first
    let masked = RE_ACCOUNT_STANDALONE.replace_all(text, "************");

    // Then apply ARN pattern (though standalone will have caught most)
    // This ensures we preserve ARN structure
    let masked = RE_ACCOUNT_IN_ARN.replace_all(&masked, "${1}************${3}");

    masked.to_string()
}

pub async fn run(script_path: &str, timescaling: f64, mask_secrets: bool) -> Result<()> {
    let data = fs::read_to_string(script_path).with_context(|| format!("reading {script_path}"))?;

    // Preprocess YAML with imports and template variables
    let processed_yaml = preprocess_yaml_v11(&data, script_path)
        .await
        .with_context(|| "preprocessing demo script YAML")?;

    let script: DemoScript = serde_yaml::from_value(processed_yaml)
        .with_context(|| "parsing preprocessed demo script")?;

    let tmp = tempdir()?;
    unpack_files(&script.files, tmp.path())?;

    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.insert("PKG_SKIP_EXECPATH_PATCH".into(), "yes".into());

    // Add current executable path for demo scripts to reference
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(exe_path) = current_exe.to_str()
    {
        env.insert("IIDY_EXE".into(), exe_path.to_string());
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

    let normalized_commands = script
        .demo
        .into_iter()
        .map(normalize_raw_command)
        .collect::<Vec<_>>();

    for command in normalized_commands {
        match command {
            DemoCommand::Shell(cmd) => {
                let substituted_cmd = substitute_iidy_command(&cmd, &iidy_exe);
                print_command(&substituted_cmd, timescaling).await?;
                exec(&substituted_cmd, tmp.path(), &env, mask_secrets)?;
            }
            DemoCommand::Silent(cmd) => {
                let substituted_cmd = substitute_iidy_command(&cmd, &iidy_exe);
                exec(&substituted_cmd, tmp.path(), &env, mask_secrets)?;
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
    // Replace 'iidy' only when it appears as a command at the beginning or after shell operators
    // Pattern matches:
    // - Start of string (with optional whitespace): ^\s*iidy\b
    // - After pipe or shell operators: [|;&(]\s*iidy\b
    let re = Regex::new(r"(^|\||\|\||&&|;|\(|\{)(\s*)(iidy)\b").unwrap();

    re.replace_all(cmd, |caps: &regex::Captures| {
        format!("{}{}{}", &caps[1], &caps[2], iidy_exe)
    })
    .to_string()
}

fn is_iidy_on_path_same_as_current_exe(current_exe_path: &str) -> bool {
    // Try to find 'iidy' executable on PATH using 'which' command
    let which_output = Command::new("which").arg("iidy").output().ok();

    if let Some(output) = which_output {
        if output.status.success() {
            let iidy_path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !iidy_path_str.is_empty() {
                // Convert both paths to canonical form for comparison
                let current_canonical = PathBuf::from(current_exe_path).canonicalize().ok();
                let iidy_canonical = PathBuf::from(&iidy_path_str).canonicalize().ok();

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
            anyhow::bail!(
                "Illegal path {}. Cannot contain parent directory references.",
                path
            );
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

fn exec(cmd: &str, cwd: &Path, env: &HashMap<String, String>, mask_secrets: bool) -> Result<()> {
    let exit_code = if mask_secrets {
        exec_with_masking(cmd, cwd, env)?
    } else {
        exec_direct(cmd, cwd, env)?
    };

    if !exit_code.success() {
        anyhow::bail!("command failed: {}", cmd);
    }

    Ok(())
}

/// Execute command directly with inherited stdout/stderr (no masking, fastest)
fn exec_direct(
    cmd: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<std::process::ExitStatus> {
    Command::new("/usr/bin/env")
        .arg("bash")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .envs(env)
        .status()
        .context("failed to execute command")
}

/// Execute command in PTY with output masking
fn exec_with_masking(
    cmd: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
) -> Result<std::process::ExitStatus> {
    let pty_system = native_pty_system();

    // Use actual terminal size for proper line wrapping
    let (term_cols, term_rows) = size().unwrap_or((80, 24));
    let pair = pty_system.openpty(PtySize {
        rows: term_rows,
        cols: term_cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Build and spawn command in PTY
    let mut cmd_builder = CommandBuilder::new("/usr/bin/env");
    cmd_builder.arg("bash");
    cmd_builder.arg("-c");
    cmd_builder.arg(cmd);
    cmd_builder.cwd(cwd);
    for (k, v) in env {
        cmd_builder.env(k, v);
    }

    let mut child = pair.slave.spawn_command(cmd_builder)?;
    drop(pair.slave); // Close slave to trigger EOF when child exits

    // Stream and mask PTY output
    stream_and_mask_pty_output(pair.master.try_clone_reader()?)?;

    // Convert portable_pty::ExitStatus to std::process::ExitStatus
    let pty_status = child.wait().context("failed to wait for child process")?;

    // Reconstruct std::process::ExitStatus from exit code
    // We need to run a dummy command with the same exit code
    let exit_code = pty_status.exit_code();
    Command::new("/usr/bin/env")
        .arg("sh")
        .arg("-c")
        .arg(format!("exit {exit_code}"))
        .status()
        .context("failed to create exit status")
}

/// Read from PTY, apply masking, write to stdout
fn stream_and_mask_pty_output(mut reader: Box<dyn Read + Send>) -> Result<()> {
    let output_handle = thread::spawn(move || {
        const BUFFER_SIZE: usize = 8192;
        const MAX_PENDING: usize = 4096;

        let mut buffer = [0u8; BUFFER_SIZE];
        let mut pending = String::new();
        let mut stdout = std::io::stdout();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    // EOF - flush remaining data
                    if !pending.is_empty() {
                        let masked = mask_aws_account_numbers(&pending);
                        let _ = stdout.write_all(masked.as_bytes());
                        let _ = stdout.flush();
                    }
                    break;
                }
                Ok(n) => {
                    if let Ok(text) = std::str::from_utf8(&buffer[0..n]) {
                        pending.push_str(text);

                        // Output complete lines for correct masking
                        if let Some(last_newline) = pending.rfind('\n') {
                            let to_output = &pending[..=last_newline];
                            let masked = mask_aws_account_numbers(to_output);
                            if stdout.write_all(masked.as_bytes()).is_ok() {
                                let _ = stdout.flush();
                            }
                            pending = pending[last_newline + 1..].to_string();
                        } else if pending.len() > MAX_PENDING {
                            // Prevent unbounded growth on very long lines
                            let masked = mask_aws_account_numbers(&pending);
                            if stdout.write_all(masked.as_bytes()).is_ok() {
                                let _ = stdout.flush();
                            }
                            pending.clear();
                        }
                    } else {
                        // Binary data - pass through unmasked (known limitation)
                        if stdout.write_all(&buffer[0..n]).is_ok() {
                            let _ = stdout.flush();
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    output_handle
        .join()
        .map_err(|e| anyhow::anyhow!("output handler thread panicked: {:?}", e))
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

        // Test that iidy in arguments is NOT substituted (the bug case)
        assert_eq!(
            substitute_iidy_command("iidy list-stacks | grep iidy-demo-foobar", exe_path),
            "/path/to/iidy list-stacks | grep iidy-demo-foobar"
        );

        // Test more argument cases where iidy should NOT be substituted
        assert_eq!(
            substitute_iidy_command("grep 'iidy-test' file.txt", exe_path),
            "grep 'iidy-test' file.txt"
        );

        assert_eq!(
            substitute_iidy_command("echo iidy-stack-name", exe_path),
            "echo iidy-stack-name"
        );

        // Test with shell operators where iidy SHOULD be substituted
        assert_eq!(
            substitute_iidy_command("echo hello; iidy help", exe_path),
            "echo hello; /path/to/iidy help"
        );

        assert_eq!(
            substitute_iidy_command("(iidy help)", exe_path),
            "(/path/to/iidy help)"
        );
    }

    #[test]
    fn test_is_iidy_on_path_same_as_current_exe() {
        // Test with non-existent path
        assert!(!is_iidy_on_path_same_as_current_exe("/non/existent/path"));

        // Test with current executable if we can determine it
        if let Ok(current_exe) = std::env::current_exe()
            && let Some(current_path) = current_exe.to_str()
        {
            // This should work regardless of whether iidy is on PATH
            let result = is_iidy_on_path_same_as_current_exe(current_path);
            // We can't assert a specific value since it depends on the environment
            // but we can ensure the function doesn't panic
            let _ = result;
        }
    }

    #[test]
    fn test_mask_aws_account_numbers_standalone() {
        // Standalone account numbers
        assert_eq!(
            mask_aws_account_numbers("account = 123456789012"),
            "account = ************"
        );

        assert_eq!(
            mask_aws_account_numbers("Account: 123456789012"),
            "Account: ************"
        );

        // With leading/trailing whitespace
        assert_eq!(
            mask_aws_account_numbers("  account = 123456789012  "),
            "  account = ************  "
        );
    }

    #[test]
    fn test_mask_aws_account_numbers_in_arns() {
        // ARNs with role
        assert_eq!(
            mask_aws_account_numbers("arn:aws:iam::123456789012:role/MyRole"),
            "arn:aws:iam::************:role/MyRole"
        );

        // ARNs with assumed-role
        assert_eq!(
            mask_aws_account_numbers("arn:aws:sts::999888777666:assumed-role/Foo"),
            "arn:aws:sts::************:assumed-role/Foo"
        );

        // ARN with user
        assert_eq!(
            mask_aws_account_numbers("arn:aws:iam::111222333444:user/admin"),
            "arn:aws:iam::************:user/admin"
        );
    }

    #[test]
    fn test_mask_multiple_account_numbers() {
        // Multiple in one line
        assert_eq!(
            mask_aws_account_numbers(
                "Account: 123456789012, ARN: arn:aws:sts::987654321098:assumed-role/Foo"
            ),
            "Account: ************, ARN: arn:aws:sts::************:assumed-role/Foo"
        );

        // Same account number twice
        assert_eq!(
            mask_aws_account_numbers("account = 123456789012 and 123456789012"),
            "account = ************ and ************"
        );
    }

    #[test]
    fn test_mask_does_not_mask_non_account_numbers() {
        // 10-digit numbers (Unix timestamps)
        assert_eq!(
            mask_aws_account_numbers("Timestamp: 1234567890"),
            "Timestamp: 1234567890"
        );

        // 11-digit numbers
        assert_eq!(
            mask_aws_account_numbers("Number: 12345678901"),
            "Number: 12345678901"
        );

        // 13-digit numbers
        assert_eq!(
            mask_aws_account_numbers("Number: 1234567890123"),
            "Number: 1234567890123"
        );

        // Numbers with separators (not 12 consecutive digits)
        assert_eq!(
            mask_aws_account_numbers("Phone: 123-456-7890"),
            "Phone: 123-456-7890"
        );
    }

    #[test]
    fn test_mask_preserves_line_structure() {
        // Multi-field line
        let input =
            "      env = production\n      region = us-west-2\n      account = 123456789012";
        let expected =
            "      env = production\n      region = us-west-2\n      account = ************";
        assert_eq!(mask_aws_account_numbers(input), expected);

        // Line with no account number
        assert_eq!(
            mask_aws_account_numbers("      env = production"),
            "      env = production"
        );
    }

    #[test]
    fn test_mask_empty_and_edge_cases() {
        // Empty string
        assert_eq!(mask_aws_account_numbers(""), "");

        // Just the account number
        assert_eq!(mask_aws_account_numbers("123456789012"), "************");

        // Account number at start of line
        assert_eq!(
            mask_aws_account_numbers("123456789012 is the account"),
            "************ is the account"
        );

        // Account number at end of line
        assert_eq!(
            mask_aws_account_numbers("The account is 123456789012"),
            "The account is ************"
        );
    }
}
