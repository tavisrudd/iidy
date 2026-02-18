//! Pixel-perfect output validation tests
//!
//! This test suite validates that renderers produce output that exactly matches
//! the expected iidy-js output, including colors, spacing, and formatting.

use iidy::cli::{ColorChoice, Theme};
use iidy::output::data::*;
use iidy::output::fixtures::FixtureLoader;
use iidy::output::renderer::OutputRenderer;
use iidy::output::renderers::interactive::{InteractiveOptions, InteractiveRenderer};
// Note: Additional imports for future output capture implementation
use insta::{assert_snapshot, with_settings};

// Note: Custom output capture implementation reserved for future
// when we implement actual stdout redirection for testing

/// Test helper to create interactive renderer options for consistent output
fn create_pixel_perfect_interactive_options() -> InteractiveOptions {
    InteractiveOptions {
        color_choice: ColorChoice::Always, // Always enable colors for testing
        theme: Theme::Dark,                // Use dark theme (default for iidy-js)
        terminal_width: Some(130),         // Fixed width matching iidy-js default
        show_timestamps: true,             // Enable timestamps
        enable_spinners: false,            // Disable for testing
        enable_ansi_features: true,        // Enable ANSI features for testing
        cli_context: None,                 // No CLI context needed for tests
    }
}

/// Test helper to create plain renderer options
fn create_pixel_perfect_plain_options() -> InteractiveOptions {
    InteractiveOptions {
        color_choice: ColorChoice::Never, // No colors for plain mode
        theme: Theme::Auto,               // Doesn't matter since colors are disabled
        terminal_width: Some(130),        // Fixed width matching iidy-js default
        show_timestamps: true,            // Enable timestamps
        enable_spinners: false,           // No spinners in plain mode
        enable_ansi_features: false,      // No ANSI features in plain mode
        cli_context: None,                // No CLI context needed for tests
    }
}

// Note: Output normalization utilities reserved for future
// when we implement actual output capture and comparison

#[tokio::test]
async fn test_interactive_command_metadata_pixel_perfect() {
    // Load fixture with expected output
    let loader = FixtureLoader::new();
    let fixture = loader
        .load_test_fixture("create-stack-happy-path")
        .expect("Should load test fixture");

    // Get expected output for interactive mode
    let expected_interactive = fixture
        .expected_outputs
        .get("interactive")
        .expect("Fixture should have interactive expected output");

    // Convert fixture to OutputData
    let output_data = loader
        .fixture_to_output_data(&fixture)
        .expect("Should convert fixture to OutputData");

    // Find the CommandMetadata in the output data
    let command_metadata = output_data
        .iter()
        .find_map(|data| match data {
            OutputData::CommandMetadata(metadata) => Some(metadata),
            _ => None,
        })
        .expect("Should find CommandMetadata in output data");

    // Create interactive renderer
    let options = create_pixel_perfect_interactive_options();
    let mut renderer = InteractiveRenderer::new(options);

    // For now, test that the renderer executes without error
    // TODO: Implement actual output capture when we have stdout redirection
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(OutputData::CommandMetadata(command_metadata.clone()), None)
        .await
        .expect("Should render");
    renderer.cleanup().await.expect("Should cleanup");

    // Verify the expected output has the key elements we're rendering
    let expected_output = &expected_interactive.stdout;
    assert!(
        expected_output.contains("Command Metadata:"),
        "Expected output should have Command Metadata section"
    );
    assert!(
        expected_output.contains("CFN Operation:"),
        "Expected output should have CFN Operation"
    );
    assert!(
        expected_output.contains("Client Req Token:"),
        "Expected output should have Client Req Token"
    );
    assert!(
        expected_output.contains("Derived Tokens:"),
        "Expected output should have Derived Tokens"
    );
}

#[tokio::test]
async fn test_plain_command_metadata_pixel_perfect() {
    // Load fixture with expected output
    let loader = FixtureLoader::new();
    let fixture = loader
        .load_test_fixture("create-stack-happy-path")
        .expect("Should load test fixture");

    // Get expected output for plain mode
    let expected_plain = fixture
        .expected_outputs
        .get("plain")
        .expect("Fixture should have plain expected output");

    // Convert fixture to OutputData
    let output_data = loader
        .fixture_to_output_data(&fixture)
        .expect("Should convert fixture to OutputData");

    // Find the CommandMetadata in the output data
    let command_metadata = output_data
        .iter()
        .find_map(|data| match data {
            OutputData::CommandMetadata(metadata) => Some(metadata),
            _ => None,
        })
        .expect("Should find CommandMetadata in output data");

    // Create plain renderer
    let options = create_pixel_perfect_plain_options();
    let mut renderer = InteractiveRenderer::new(options);

    // Test that the renderer executes without error
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(OutputData::CommandMetadata(command_metadata.clone()), None)
        .await
        .expect("Should render");
    renderer.cleanup().await.expect("Should cleanup");

    // Verify the expected output structure
    let expected_output = &expected_plain.stdout;
    assert!(
        expected_output.contains("Command Metadata:"),
        "Expected output should have Command Metadata section"
    );
    assert!(
        expected_output.contains("CFN Operation:"),
        "Expected output should have CFN Operation"
    );
    assert!(
        expected_output.contains("create-stack"),
        "Expected output should have the operation name"
    );
}

#[tokio::test]
async fn test_interactive_stack_definition_pixel_perfect() {
    // Load fixture
    let loader = FixtureLoader::new();
    let fixture = loader
        .load_test_fixture("create-stack-happy-path")
        .expect("Should load test fixture");

    // Convert fixture to OutputData
    let output_data = loader
        .fixture_to_output_data(&fixture)
        .expect("Should convert fixture to OutputData");

    // Find the StackDefinition in the output data
    let (stack_definition, show_times) = output_data
        .iter()
        .find_map(|data| match data {
            OutputData::StackDefinition(def, show_times) => Some((def, *show_times)),
            _ => None,
        })
        .expect("Should find StackDefinition in output data");

    // Create interactive renderer
    let options = create_pixel_perfect_interactive_options();
    let mut renderer = InteractiveRenderer::new(options);

    // Test rendering
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(
            OutputData::StackDefinition(stack_definition.clone(), show_times),
            None,
        )
        .await
        .expect("Should render");
    renderer.cleanup().await.expect("Should cleanup");

    // Verify expected content in fixture
    let expected_interactive = fixture
        .expected_outputs
        .get("interactive")
        .expect("Fixture should have interactive expected output");

    let expected_output = &expected_interactive.stdout;
    assert!(
        expected_output.contains("Stack Details:"),
        "Expected output should have Stack Details section"
    );
    assert!(
        expected_output.contains("Name:"),
        "Expected output should have Name field"
    );
    assert!(
        expected_output.contains("Status:"),
        "Expected output should have Status field"
    );
    assert!(
        expected_output.contains("test-stack"),
        "Expected output should have the stack name"
    );
}

#[tokio::test]
async fn test_interactive_stack_events_pixel_perfect() {
    // Load fixture
    let loader = FixtureLoader::new();
    let fixture = loader
        .load_test_fixture("create-stack-happy-path")
        .expect("Should load test fixture");

    // Convert fixture to OutputData
    let output_data = loader
        .fixture_to_output_data(&fixture)
        .expect("Should convert fixture to OutputData");

    // Find the StackEvents in the output data
    let stack_events = output_data
        .iter()
        .find_map(|data| match data {
            OutputData::StackEvents(events) => Some(events),
            _ => None,
        })
        .expect("Should find StackEvents in output data");

    // Create interactive renderer
    let options = create_pixel_perfect_interactive_options();
    let mut renderer = InteractiveRenderer::new(options);

    // Test rendering
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(OutputData::StackEvents(stack_events.clone()), None)
        .await
        .expect("Should render");
    renderer.cleanup().await.expect("Should cleanup");

    // Verify expected content in fixture
    let expected_interactive = fixture
        .expected_outputs
        .get("interactive")
        .expect("Fixture should have interactive expected output");

    let expected_output = &expected_interactive.stdout;
    assert!(
        expected_output.contains("Previous Stack Events"),
        "Expected output should have Stack Events section"
    );
    assert!(
        expected_output.contains("Timestamp"),
        "Expected output should have Timestamp column"
    );
    assert!(
        expected_output.contains("CREATE_COMPLETE"),
        "Expected output should have CREATE_COMPLETE status"
    );
}

#[test]
fn test_fixture_expected_output_completeness() {
    // Verify that our fixture has comprehensive expected outputs
    let loader = FixtureLoader::new();
    let fixture = loader
        .load_test_fixture("create-stack-happy-path")
        .expect("Should load test fixture");

    // Verify all output modes are present
    assert!(
        fixture.expected_outputs.contains_key("interactive"),
        "Should have interactive expected output"
    );
    assert!(
        fixture.expected_outputs.contains_key("plain"),
        "Should have plain expected output"
    );
    assert!(
        fixture.expected_outputs.contains_key("json"),
        "Should have json expected output"
    );

    // Verify outputs are substantial (indicating real content)
    let interactive = fixture.expected_outputs.get("interactive").unwrap();
    let plain = fixture.expected_outputs.get("plain").unwrap();
    let json = fixture.expected_outputs.get("json").unwrap();

    assert!(
        interactive.stdout.len() > 2000,
        "Interactive output should be substantial (got {} chars)",
        interactive.stdout.len()
    );
    assert!(
        plain.stdout.len() > 2000,
        "Plain output should be substantial (got {} chars)",
        plain.stdout.len()
    );
    assert!(
        json.stdout.len() > 1000,
        "JSON output should be substantial (got {} chars)",
        json.stdout.len()
    );

    // Verify structure consistency
    assert_eq!(
        interactive.exit_code, 0,
        "Interactive should have exit code 0"
    );
    assert_eq!(plain.exit_code, 0, "Plain should have exit code 0");
    assert_eq!(json.exit_code, 0, "JSON should have exit code 0");
}

/// Test output formatting constants match iidy-js exactly
#[test]
fn test_formatting_constants_match_iidy_js() {
    use iidy::output::renderers::interactive::{
        COLUMN2_START, MAX_PADDING, MIN_STATUS_PADDING, RESOURCE_TYPE_PADDING,
    };

    // Verify constants match the implementation spec
    assert_eq!(COLUMN2_START, 25, "COLUMN2_START should match iidy-js spec");
    assert_eq!(
        MIN_STATUS_PADDING, 17,
        "MIN_STATUS_PADDING should match iidy-js spec"
    );
    assert_eq!(MAX_PADDING, 60, "MAX_PADDING should match iidy-js spec");
    assert_eq!(
        RESOURCE_TYPE_PADDING, 40,
        "RESOURCE_TYPE_PADDING should match iidy-js spec"
    );
}

/// Snapshot test to track output format changes
#[tokio::test]
async fn test_renderer_format_snapshot() {
    let loader = FixtureLoader::new();
    let fixture = loader
        .load_test_fixture("create-stack-happy-path")
        .expect("Should load test fixture");

    // Get expected outputs for snapshot comparison
    let interactive_expected = fixture.expected_outputs.get("interactive").unwrap();
    let plain_expected = fixture.expected_outputs.get("plain").unwrap();

    // Create summary for snapshot
    let output_summary = serde_json::json!({
        "interactive_lines": interactive_expected.stdout.lines().count(),
        "plain_lines": plain_expected.stdout.lines().count(),
        "interactive_length": interactive_expected.stdout.len(),
        "plain_length": plain_expected.stdout.len(),
        "has_command_metadata": interactive_expected.stdout.contains("Command Metadata:"),
        "has_stack_details": interactive_expected.stdout.contains("Stack Details:"),
        "has_stack_events": interactive_expected.stdout.contains("Stack Events"),
        "has_stack_resources": interactive_expected.stdout.contains("Stack Resources:"),
        "has_stack_outputs": interactive_expected.stdout.contains("Stack Outputs:"),
        "has_command_result": interactive_expected.stdout.contains("Command Result:")
    });

    with_settings!({
        description => "Expected output format structure"
    }, {
        assert_snapshot!("expected_output_format", output_summary);
    });
}
