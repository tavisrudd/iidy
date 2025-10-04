//! JSON renderer for structured machine-readable output
//!
//! This renderer outputs data in JSON Lines (JSONL) format, where each
//! piece of rendered data becomes a JSON object with type, timestamp, and data fields.
//! This format is ideal for automation, logging, and integration with other tools.

use crate::output::data::*;
use crate::output::renderer::OutputRenderer;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use std::io::{self, Write};

/// Configuration options for JSON rendering
#[derive(Debug, Clone)]
pub struct JsonOptions {
    /// Whether to include timestamps in output
    pub include_timestamps: bool,
    /// Whether to pretty-print JSON (for debugging)
    pub pretty_print: bool,
    /// Whether to include type information
    pub include_type: bool,
}

impl Default for JsonOptions {
    fn default() -> Self {
        Self {
            include_timestamps: true,
            pretty_print: false, // JSONL format should be compact by default
            include_type: true,
        }
    }
}

/// JSON renderer that outputs structured JSONL data
pub struct JsonRenderer {
    options: JsonOptions,
}

impl JsonRenderer {
    pub fn new(options: JsonOptions) -> Self {
        Self { options }
    }

    /// Output raw JSON data without metadata wrapping
    fn output_raw_json(&self, data: &(impl serde::Serialize + ?Sized)) -> Result<()> {
        let json_output = serde_json::to_string_pretty(data)
            .map_err(|e| anyhow::anyhow!("Failed to serialize data: {}", e))?;
        println!("{}", json_output);
        io::stdout().flush()?;
        Ok(())
    }

    /// Output a JSON object for the given data
    fn output_json(&self, type_name: &str, data: &(impl serde::Serialize + ?Sized)) -> Result<()> {
        let json_obj = if self.options.include_timestamps && self.options.include_type {
            json!({
                "type": type_name,
                "timestamp": Utc::now().to_rfc3339(),
                "data": data
            })
        } else if self.options.include_type {
            json!({
                "type": type_name,
                "data": data
            })
        } else if self.options.include_timestamps {
            json!({
                "timestamp": Utc::now().to_rfc3339(),
                "data": data
            })
        } else {
            json!(data)
        };

        let output = if self.options.pretty_print {
            serde_json::to_string_pretty(&json_obj)?
        } else {
            serde_json::to_string(&json_obj)?
        };

        println!("{}", output);
        io::stdout().flush()?;

        Ok(())
    }
}

#[async_trait]
impl OutputRenderer for JsonRenderer {
    async fn init(&mut self) -> Result<()> {
        // JSON renderer doesn't need initialization
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Flush any remaining output
        io::stdout().flush()?;
        Ok(())
    }

    /// Render OutputData as JSON (ignores buffer for ordering)
    async fn render_output_data(
        &mut self,
        data: OutputData,
        _buffer: Option<&std::collections::VecDeque<crate::output::data::OutputData>>,
    ) -> Result<()> {
        match data {
            OutputData::CommandMetadata(ref metadata) => {
                self.render_command_metadata(metadata).await
            }
            OutputData::StackDefinition(ref def, show_times) => {
                self.render_stack_definition(def, show_times).await
            }
            OutputData::StackEvents(ref events) => self.render_stack_events(events).await,
            OutputData::StackContents(ref contents) => self.render_stack_contents(contents).await,
            OutputData::StatusUpdate(ref update) => self.render_status_update(update).await,
            OutputData::CommandResult(ref result) => self.render_command_result(result).await,
            OutputData::FinalCommandSummary(ref summary) => {
                self.render_final_command_summary(summary).await
            }
            OutputData::StackList(ref list) => self.render_stack_list(list).await,
            OutputData::ChangeSetResult(ref result) => self.render_changeset_result(result).await,
            OutputData::StackDrift(ref drift) => self.render_stack_drift(drift).await,
            OutputData::Error(ref error) => self.render_error(error).await,
            OutputData::TokenInfo(ref token) => self.render_token_info(token).await,
            OutputData::NewStackEvents(ref events) => self.render_new_stack_events(events).await,
            OutputData::OperationComplete(ref info) => self.render_operation_complete(info).await,
            OutputData::InactivityTimeout(ref info) => self.render_inactivity_timeout(info).await,
            OutputData::ConfirmationPrompt(request) => {
                self.render_confirmation_prompt(request).await
            }
            OutputData::StackChangeDetails(ref details) => {
                self.render_stack_change_details(details).await
            }
            OutputData::StackAbsentInfo(ref info) => self.render_stack_absent_info(info).await,
            OutputData::CostEstimate(ref estimate) => self.render_cost_estimate(estimate).await,
            OutputData::StackTemplate(ref template) => self.render_stack_template(template).await,
            OutputData::ApprovalRequestResult(ref result) => {
                self.render_approval_request_result(result).await
            }
            OutputData::TemplateValidation(ref validation) => {
                self.render_template_validation(validation).await
            }
            OutputData::ApprovalStatus(ref status) => self.render_approval_status(status).await,
            OutputData::TemplateDiff(ref diff) => self.render_template_diff(diff).await,
            OutputData::ApprovalResult(ref result) => self.render_approval_result(result).await,
        }
    }
}

impl JsonRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        self.output_json("command_metadata", data)
    }

    async fn render_stack_definition(
        &mut self,
        data: &StackDefinition,
        show_times: bool,
    ) -> Result<()> {
        // Include the show_times flag in the JSON output
        let stack_data = json!({
            "stack_definition": data,
            "show_times": show_times
        });
        self.output_json("stack_definition", &stack_data)
    }

    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()> {
        self.output_json("stack_events", data)
    }

    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()> {
        self.output_json("stack_contents", data)
    }

    async fn render_status_update(&mut self, data: &StatusUpdate) -> Result<()> {
        self.output_json("status_update", data)
    }

    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()> {
        self.output_json("command_result", data)
    }

    async fn render_final_command_summary(
        &mut self,
        data: &crate::output::data::FinalCommandSummary,
    ) -> Result<()> {
        self.output_json("final_command_summary", data)
    }

    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()> {
        if data.query_mode {
            // For query mode, output raw JSON array of stacks (matching --query behavior)
            self.output_raw_json(&data.stacks)
        } else {
            // Normal structured output
            self.output_json("stack_list", data)
        }
    }

    async fn render_changeset_result(&mut self, data: &ChangeSetCreationResult) -> Result<()> {
        self.output_json("changeset_result", data)
    }

    async fn render_stack_drift(&mut self, data: &StackDrift) -> Result<()> {
        self.output_json("stack_drift", data)
    }

    async fn render_error(&mut self, data: &ErrorInfo) -> Result<()> {
        self.output_json("error", data)
    }

    async fn render_token_info(&mut self, data: &TokenInfo) -> Result<()> {
        self.output_json("token_info", data)
    }

    async fn render_new_stack_events(
        &mut self,
        events: &[crate::output::data::StackEventWithTiming],
    ) -> Result<()> {
        self.output_json("new_stack_events", events)
    }

    async fn render_operation_complete(
        &mut self,
        info: &crate::output::data::OperationCompleteInfo,
    ) -> Result<()> {
        self.output_json("operation_complete", info)
    }

    async fn render_inactivity_timeout(
        &mut self,
        info: &crate::output::data::InactivityTimeoutInfo,
    ) -> Result<()> {
        self.output_json("inactivity_timeout", info)
    }

    async fn render_confirmation_prompt(
        &mut self,
        mut request: crate::output::data::ConfirmationRequest,
    ) -> Result<()> {
        // JSON mode: output confirmation event but don't interact
        let confirmation_event = serde_json::json!({
            "type": "confirmation_required",
            "message": request.message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "response": "declined_non_interactive"
        });

        println!("{}", confirmation_event.to_string());

        // Send response back to command handler via channel
        if let Some(response_tx) = request.response_tx.take() {
            let _ = response_tx.send(false); // Always decline in JSON mode
        }

        Ok(())
    }

    async fn render_stack_change_details(
        &mut self,
        data: &crate::output::data::StackChangeDetails,
    ) -> Result<()> {
        self.output_json("stack_change_details", data)
    }

    async fn render_stack_absent_info(
        &mut self,
        data: &crate::output::data::StackAbsentInfo,
    ) -> Result<()> {
        self.output_json("stack_absent_info", data)
    }

    async fn render_cost_estimate(
        &mut self,
        data: &crate::output::data::CostEstimate,
    ) -> Result<()> {
        self.output_json("cost_estimate", data)
    }

    async fn render_stack_template(
        &mut self,
        data: &crate::output::data::StackTemplate,
    ) -> Result<()> {
        // For JSON mode, the template should behave like the interactive mode
        // Print stderr lines to stderr and template body to stdout
        // This maintains the same external behavior
        for line in &data.stderr_lines {
            eprintln!("{}", line);
        }
        println!("{}", data.template_body);
        Ok(())
    }

    async fn render_approval_request_result(
        &mut self,
        data: &crate::output::data::ApprovalRequestResult,
    ) -> Result<()> {
        self.output_json("approval_request_result", data)
    }

    async fn render_template_validation(
        &mut self,
        data: &crate::output::data::TemplateValidation,
    ) -> Result<()> {
        self.output_json("template_validation", data)
    }

    async fn render_approval_status(
        &mut self,
        data: &crate::output::data::ApprovalStatus,
    ) -> Result<()> {
        self.output_json("approval_status", data)
    }

    async fn render_template_diff(
        &mut self,
        data: &crate::output::data::TemplateDiff,
    ) -> Result<()> {
        self.output_json("template_diff", data)
    }

    async fn render_approval_result(
        &mut self,
        data: &crate::output::data::ApprovalResult,
    ) -> Result<()> {
        self.output_json("approval_result", data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_sample_command_metadata() -> CommandMetadata {
        CommandMetadata {
            iidy_environment: "test".to_string(),
            region: "us-east-1".to_string(),
            profile: Some("test-profile".to_string()),
            cli_arguments: [
                ("template".to_string(), "template.yaml".to_string()),
                ("argsfile".to_string(), "stack-args.yaml".to_string()),
            ]
            .into_iter()
            .collect(),
            iam_service_role: None,
            current_iam_principal: "arn:aws:iam::123456789012:user/test-user".to_string(),
            credential_source: "profile 'test-profile' (default)".to_string(),
            iidy_version: "2.0.0".to_string(),
            primary_token: TokenInfo {
                value: "test-token-001".to_string(),
                source: TokenSource::AutoGenerated,
                operation_id: "create-stack-001".to_string(),
            },
            derived_tokens: vec![],
        }
    }

    #[tokio::test]
    async fn test_json_renderer_creation() {
        let options = JsonOptions::default();
        let _renderer = JsonRenderer::new(options);

        // Basic creation test
        assert!(true); // If we reach here, creation succeeded
    }

    #[tokio::test]
    async fn test_json_renderer_lifecycle() {
        let options = JsonOptions::default();
        let mut renderer = JsonRenderer::new(options);

        // Test initialization
        renderer.init().await.expect("Should initialize");

        // Test cleanup
        renderer.cleanup().await.expect("Should cleanup");
    }

    #[tokio::test]
    async fn test_command_metadata_rendering() {
        let options = JsonOptions::default();
        let mut renderer = JsonRenderer::new(options);

        let metadata = create_sample_command_metadata();

        // This should not panic or error
        renderer
            .render_command_metadata(&metadata)
            .await
            .expect("Should render command metadata");
    }

    #[tokio::test]
    async fn test_stack_definition_rendering() {
        let options = JsonOptions::default();
        let mut renderer = JsonRenderer::new(options);

        let stack_def = StackDefinition {
            name: "test-stack".to_string(),
            stackset_name: None,
            description: Some("Test stack".to_string()),
            status: "CREATE_COMPLETE".to_string(),
            status_reason: None,
            capabilities: vec!["CAPABILITY_IAM".to_string()],
            service_role: None,
            tags: HashMap::new(),
            parameters: HashMap::new(),
            disable_rollback: false,
            termination_protection: false,
            creation_time: Some(Utc::now()),
            last_updated_time: None,
            timeout_in_minutes: Some(30),
            notification_arns: vec![],
            stack_policy: None,
            arn: "arn:aws:cloudformation:us-east-1:123456789012:stack/test-stack/id".to_string(),
            console_url: "https://console.aws.amazon.com/cloudformation".to_string(),
            region: "us-east-1".to_string(),
        };

        renderer
            .render_stack_definition(&stack_def, true)
            .await
            .expect("Should render stack definition");
    }

    #[tokio::test]
    async fn test_status_update_rendering() {
        let options = JsonOptions::default();
        let mut renderer = JsonRenderer::new(options);

        let status = StatusUpdate {
            message: "Test status update".to_string(),
            timestamp: Utc::now(),
            level: StatusLevel::Info,
        };

        renderer
            .render_status_update(&status)
            .await
            .expect("Should render status update");
    }

    #[tokio::test]
    async fn test_command_result_rendering() {
        let options = JsonOptions::default();
        let mut renderer = JsonRenderer::new(options);

        let result = CommandResult {
            success: true,
            elapsed_seconds: 120,
            message: Some("Operation completed".to_string()),
            exit_code: 0,
        };

        renderer
            .render_command_result(&result)
            .await
            .expect("Should render command result");
    }

    #[tokio::test]
    async fn test_error_rendering() {
        let options = JsonOptions::default();
        let mut renderer = JsonRenderer::new(options);

        let error = ErrorInfo {
            error_type: "TestError".to_string(),
            message: "Test error message".to_string(),
            timestamp: Utc::now(),
            suggestions: vec!["Try again".to_string()],
            error_details: ErrorDetails::Generic(Some("Error details".to_string())),
        };

        renderer
            .render_error(&error)
            .await
            .expect("Should render error");
    }

    #[tokio::test]
    async fn test_json_options_configurations() {
        // Test with timestamps disabled
        let options = JsonOptions {
            include_timestamps: false,
            pretty_print: false,
            include_type: true,
        };
        let mut renderer = JsonRenderer::new(options);
        let metadata = create_sample_command_metadata();

        renderer
            .render_command_metadata(&metadata)
            .await
            .expect("Should render without timestamps");

        // Test with pretty printing enabled
        let options = JsonOptions {
            include_timestamps: true,
            pretty_print: true,
            include_type: true,
        };
        let mut renderer = JsonRenderer::new(options);

        renderer
            .render_command_metadata(&metadata)
            .await
            .expect("Should render with pretty printing");

        // Test with type information disabled
        let options = JsonOptions {
            include_timestamps: true,
            pretty_print: false,
            include_type: false,
        };
        let mut renderer = JsonRenderer::new(options);

        renderer
            .render_command_metadata(&metadata)
            .await
            .expect("Should render without type information");
    }
}
