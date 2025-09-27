use anyhow::Result;
use crate::output::{DynamicOutputManager, OutputData, aws_conversion::convert_aws_error_to_error_info};

/// Helper function to handle AWS errors with consistent pattern
pub async fn handle_aws_error<T>(result: Result<T>, output_manager: &mut DynamicOutputManager) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) => {
            let error_info = convert_aws_error_to_error_info(&e);
            output_manager.render(OutputData::Error(error_info)).await?;
            Ok(None) // Signal failure
        }
    }
}