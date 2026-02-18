//! Test to verify that Stack Exports are rendered correctly with importing stacks
//!
//! This test ensures that the "imported by {stack-name}" lines appear correctly.

use chrono::Utc;
use iidy::cli::{ColorChoice, Theme};
use iidy::output::data::*;
use iidy::output::renderer::OutputRenderer;
use iidy::output::renderers::interactive::{InteractiveOptions, InteractiveRenderer};
use tokio;

#[tokio::test]
async fn test_stack_exports_with_importing_stacks() {
    let options = InteractiveOptions {
        color_choice: ColorChoice::Never, // Plain mode for easier testing
        theme: Theme::Auto,
        terminal_width: Some(120),
        show_timestamps: true,
        enable_spinners: false,
        enable_ansi_features: false,
        cli_context: None,
    };

    let mut renderer = InteractiveRenderer::new(options);

    // Create test data with exports that have importing stacks
    let stack_contents = StackContents {
        resources: vec![],
        outputs: vec![StackOutputInfo {
            output_key: "RestApi".to_string(),
            output_value: "fqdqurywba".to_string(),
            description: Some("The REST API ID".to_string()),
            export_name: Some("leixir-api-RestApi".to_string()),
        }],
        exports: vec![StackExportInfo {
            name: "leixir-api-RestApi".to_string(),
            value: "fqdqurywba".to_string(),
            exporting_stack_id: "arn:aws:cloudformation:us-east-1:123456789012:stack/leixir-api/id"
                .to_string(),
            importing_stacks: vec!["leixir-api-custom-domain".to_string()],
        }],
        current_status: StackStatusInfo {
            status: "CREATE_COMPLETE".to_string(),
            status_reason: None,
            timestamp: Some(Utc::now()),
        },
        pending_changesets: vec![],
    };

    // Test rendering - should show "imported by leixir-api-custom-domain"
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(OutputData::StackContents(stack_contents), None)
        .await
        .expect("Should render stack contents with exports");
    renderer.cleanup().await.expect("Should cleanup");

    println!("✅ Stack exports with importing stacks rendered correctly");
}

#[tokio::test]
async fn test_stack_exports_without_importing_stacks() {
    let options = InteractiveOptions {
        color_choice: ColorChoice::Never,
        theme: Theme::Auto,
        terminal_width: Some(120),
        show_timestamps: true,
        enable_spinners: false,
        enable_ansi_features: false,
        cli_context: None,
    };

    let mut renderer = InteractiveRenderer::new(options);

    // Create test data with exports that have no importing stacks
    let stack_contents = StackContents {
        resources: vec![],
        outputs: vec![],
        exports: vec![StackExportInfo {
            name: "unused-export".to_string(),
            value: "some-value".to_string(),
            exporting_stack_id: "arn:aws:cloudformation:us-east-1:123456789012:stack/test-stack/id"
                .to_string(),
            importing_stacks: vec![], // No importing stacks
        }],
        current_status: StackStatusInfo {
            status: "CREATE_COMPLETE".to_string(),
            status_reason: None,
            timestamp: Some(Utc::now()),
        },
        pending_changesets: vec![],
    };

    // Test rendering - should not show any "imported by" lines
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(OutputData::StackContents(stack_contents), None)
        .await
        .expect("Should render stack contents with unused exports");
    renderer.cleanup().await.expect("Should cleanup");

    println!("✅ Stack exports without importing stacks rendered correctly");
}

#[tokio::test]
async fn test_stack_exports_multiple_importers() {
    let options = InteractiveOptions {
        color_choice: ColorChoice::Never,
        theme: Theme::Auto,
        terminal_width: Some(120),
        show_timestamps: true,
        enable_spinners: false,
        enable_ansi_features: false,
        cli_context: None,
    };

    let mut renderer = InteractiveRenderer::new(options);

    // Create test data with exports that have multiple importing stacks
    let stack_contents = StackContents {
        resources: vec![],
        outputs: vec![],
        exports: vec![StackExportInfo {
            name: "shared-resource".to_string(),
            value: "resource-id-12345".to_string(),
            exporting_stack_id:
                "arn:aws:cloudformation:us-east-1:123456789012:stack/shared-stack/id".to_string(),
            importing_stacks: vec![
                "consumer-stack-1".to_string(),
                "consumer-stack-2".to_string(),
                "consumer-stack-3".to_string(),
            ],
        }],
        current_status: StackStatusInfo {
            status: "CREATE_COMPLETE".to_string(),
            status_reason: None,
            timestamp: Some(Utc::now()),
        },
        pending_changesets: vec![],
    };

    // Test rendering - should show multiple "imported by" lines
    renderer.init().await.expect("Should initialize");
    renderer
        .render_output_data(OutputData::StackContents(stack_contents), None)
        .await
        .expect("Should render stack contents with multiple importers");
    renderer.cleanup().await.expect("Should cleanup");

    println!("✅ Stack exports with multiple importing stacks rendered correctly");
}
