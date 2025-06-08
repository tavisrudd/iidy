/// Spinner demonstration module showcasing ora-like API
use crate::color::{ColorExt, ProgressManager, SpinnerStyle};
use std::thread;
use std::time::Duration;

/// Demonstrate spinner functionality with real CloudFormation-like operations
pub fn run_spinner_demo() {
    println!("{}", "Ora-like Spinner Demo for iidy".bold_text());
    println!();
    
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        println!("{}", "⚠ Not running in a TTY - spinners will show as text updates".warning());
        println!("  Try running in a real terminal to see animated spinners!");
        println!();
    } else {
        println!("{}", "✓ TTY detected - showing animated spinners".success());
        println!();
    }
    
    // Show different spinner styles first
    demonstrate_spinner_styles();
    println!();
    
    // Show ora-like API methods
    demonstrate_ora_api();
    println!();
    
    // Simulate a complete CloudFormation stack creation workflow
    simulate_stack_creation();
    println!();
    
    println!("{}", "Ora-like spinner demonstration complete!".success());
}

fn demonstrate_spinner_styles() {
    println!("{}", "Available Spinner Styles:".bold_text());
    
    let styles = vec![
        (SpinnerStyle::Dots, "Dots (default braille pattern)"),
        (SpinnerStyle::Dots12, "Dots12 (extended braille - like ora)"),
        (SpinnerStyle::Line, "Line (growing line animation)"),
        (SpinnerStyle::Arrow, "Arrow (directional arrows)"),
        (SpinnerStyle::Pulse, "Pulse (simple on/off)"),
    ];
    
    for (style, description) in styles {
        println!();
        let spinner = ProgressManager::with_style(style, &format!("Testing {} style...", format!("{:?}", style).muted()));
        thread::sleep(Duration::from_millis(2000));
        spinner.succeed(&format!("{} - {}", format!("{:?}", style).bold_text(), description));
    }
}

fn demonstrate_ora_api() {
    println!("{}", "Ora-like API Methods:".bold_text());
    println!();
    
    // succeed() method
    println!("{}:", "1. spinner.succeed()".info());
    let spinner = ProgressManager::with_style(SpinnerStyle::Dots12, "Operation in progress...");
    thread::sleep(Duration::from_millis(1500));
    spinner.succeed("Operation completed successfully");
    println!();
    
    // fail() method
    println!("{}:", "2. spinner.fail()".info());
    let spinner = ProgressManager::with_style(SpinnerStyle::Dots, "Attempting risky operation...");
    thread::sleep(Duration::from_millis(1200));
    spinner.fail("Operation failed due to insufficient permissions");
    println!();
    
    // warn() method
    println!("{}:", "3. spinner.warn()".info());
    let spinner = ProgressManager::with_style(SpinnerStyle::Line, "Checking configuration...");
    thread::sleep(Duration::from_millis(1000));
    spinner.warn("Configuration has deprecated settings");
    println!();
    
    // info() method
    println!("{}:", "4. spinner.info()".info());
    let spinner = ProgressManager::with_style(SpinnerStyle::Arrow, "Gathering system information...");
    thread::sleep(Duration::from_millis(1300));
    spinner.info("System information collected");
    println!();
    
    // Dynamic text updates (ora.text = "...")
    println!("{}:", "5. Dynamic text updates".info());
    let spinner = ProgressManager::with_style(SpinnerStyle::Pulse, "Starting...");
    thread::sleep(Duration::from_millis(500));
    spinner.set_text("Processing step 1 of 3...");
    thread::sleep(Duration::from_millis(600));
    spinner.set_text("Processing step 2 of 3...");
    thread::sleep(Duration::from_millis(600));
    spinner.set_text("Processing step 3 of 3...");
    thread::sleep(Duration::from_millis(600));
    spinner.succeed("All steps completed");
    println!();
}

fn simulate_stack_creation() {
    println!("{}", "CloudFormation Stack Creation with Ora-like API:".bold_text());
    println!();
    
    // Use dots12 style like the original ora example
    let spinner = ProgressManager::with_style(SpinnerStyle::Dots12, "Validating CloudFormation template syntax...");
    thread::sleep(Duration::from_millis(1000));
    
    spinner.set_text("Checking IAM permissions and capabilities...");
    thread::sleep(Duration::from_millis(800));
    
    spinner.set_text("Validating parameter constraints...");
    thread::sleep(Duration::from_millis(600));
    
    spinner.set_text("Creating CloudFormation stack...");
    thread::sleep(Duration::from_millis(1200));
    
    spinner.set_text("Stack CREATE_IN_PROGRESS - Creating resources...");
    thread::sleep(Duration::from_millis(1000));
    
    spinner.set_text("Creating IAM Role (MyRole)...");
    thread::sleep(Duration::from_millis(800));
    
    spinner.set_text("Creating S3 Bucket (MyBucket-abc123)...");
    thread::sleep(Duration::from_millis(900));
    
    spinner.set_text("Creating Lambda Function (MyFunction)...");
    thread::sleep(Duration::from_millis(1200));
    
    spinner.set_text("Configuring resource dependencies...");
    thread::sleep(Duration::from_millis(700));
    
    spinner.set_text("Finalizing stack creation...");
    thread::sleep(Duration::from_millis(500));
    
    // Finish with success - ora-style!
    spinner.succeed("Stack 'my-demo-stack' created successfully!");
}

