/// Error ID system for iidy YAML preprocessing
/// 
/// Format: IY + Category + Number
/// Categories:
/// - 1xxx: YAML Syntax & Parsing  
/// - 2xxx: Variable & Scope Errors
/// - 3xxx: Import & Loading Errors
/// - 4xxx: Tag Syntax & Structure Errors
/// - 5xxx: Type & Validation Errors
/// - 6xxx: Template & Handlebars Errors
/// - 7xxx: CloudFormation Specific
/// - 8xxx: Configuration & Setup
/// - 9xxx: Internal & System Errors

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorId {
    // 1xxx - YAML Syntax & Parsing
    InvalidYamlSyntax = 1001,
    YamlVersionMismatch = 1002,
    UnsupportedYamlFeature = 1003,
    MalformedYamlStructure = 1004,
    YamlMergeKeyUsage = 1005,
    
    // 2xxx - Variable & Scope Errors
    VariableNotFound = 2001,
    VariableNameCollision = 2002,
    InvalidVariableName = 2003,
    CircularVariableReference = 2004,
    VariableOutOfScope = 2005,
    
    // 3xxx - Import & Loading Errors
    ImportFileNotFound = 3001,
    ImportUrlUnreachable = 3002,
    ImportAuthenticationFailure = 3003,
    ImportCircularDependency = 3004,
    ImportFormatNotSupported = 3005,
    EnvironmentVariableNotFound = 3006,
    GitCommandFailure = 3007,
    S3AccessDenied = 3008,
    SsmParameterNotFound = 3009,
    CloudFormationStackNotFound = 3010,
    
    // 4xxx - Tag Syntax & Structure Errors
    UnknownPreprocessingTag = 4001,
    MissingRequiredTagField = 4002,
    InvalidTagFieldValue = 4003,
    IncompatibleTagCombination = 4004,
    TagSyntaxError = 4005,
    
    // 5xxx - Type & Validation Errors
    TypeMismatchInOperation = 5001,
    InvalidArrayOperation = 5002,
    InvalidObjectOperation = 5003,
    DivisionByZero = 5004,
    InvalidComparison = 5005,
    StringOperationOnNonString = 5006,
    
    // 6xxx - Template & Handlebars Errors
    HandlebarsSyntaxError = 6001,
    UnknownHandlebarsHelper = 6002,
    HandlebarsHelperArgumentError = 6003,
    TemplateCompilationFailure = 6004,
    TemplateExecutionError = 6005,
    
    // 7xxx - CloudFormation Specific
    InvalidCloudFormationIntrinsic = 7001,
    CloudFormationReferenceError = 7002,
    CloudFormationDependencyIssue = 7003,
    CloudFormationTemplateSizeLimit = 7004,
    
    // 8xxx - Configuration & Setup
    InvalidCommandLineArgument = 8001,
    MissingRequiredConfiguration = 8002,
    ConfigurationFileNotFound = 8003,
    AwsCredentialsNotConfigured = 8004,
    UnsupportedFileFormat = 8005,
    
    // 9xxx - Internal & System Errors
    InternalProcessingError = 9001,
    MemoryAllocationFailure = 9002,
    FileSystemPermissionDenied = 9003,
    NetworkConnectivityIssue = 9004,
    UnexpectedSystemError = 9005,
}

impl ErrorId {
    /// Get the error code string (e.g., "IY2001")
    pub fn code(&self) -> String {
        format!("IY{:04}", *self as u16)
    }
    
    /// Get the category name for this error
    pub fn category(&self) -> &'static str {
        match *self as u16 {
            1000..=1999 => "YAML Syntax & Parsing",
            2000..=2999 => "Variable & Scope",
            3000..=3999 => "Import & Loading",
            4000..=4999 => "Tag Syntax & Structure", 
            5000..=5999 => "Type & Validation",
            6000..=6999 => "Template & Handlebars",
            7000..=7999 => "CloudFormation Specific",
            8000..=8999 => "Configuration & Setup",
            9000..=9999 => "Internal & System",
            _ => "Unknown"
        }
    }
    
    /// Get the short description for this error
    pub fn description(&self) -> &'static str {
        match self {
            // 1xxx - YAML Syntax & Parsing
            ErrorId::InvalidYamlSyntax => "Invalid YAML syntax",
            ErrorId::YamlVersionMismatch => "YAML version mismatch",
            ErrorId::UnsupportedYamlFeature => "Unsupported YAML feature",
            ErrorId::MalformedYamlStructure => "Malformed YAML structure",
            ErrorId::YamlMergeKeyUsage => "YAML merge key not supported in 1.2",
            
            // 2xxx - Variable & Scope Errors
            ErrorId::VariableNotFound => "Variable not found",
            ErrorId::VariableNameCollision => "Variable name collision", 
            ErrorId::InvalidVariableName => "Invalid variable name",
            ErrorId::CircularVariableReference => "Circular variable reference",
            ErrorId::VariableOutOfScope => "Variable access out of scope",
            
            // 3xxx - Import & Loading Errors
            ErrorId::ImportFileNotFound => "Import file not found",
            ErrorId::ImportUrlUnreachable => "Import URL unreachable",
            ErrorId::ImportAuthenticationFailure => "Import authentication failure",
            ErrorId::ImportCircularDependency => "Import circular dependency",
            ErrorId::ImportFormatNotSupported => "Import format not supported",
            ErrorId::EnvironmentVariableNotFound => "Environment variable not found",
            ErrorId::GitCommandFailure => "Git command failure",
            ErrorId::S3AccessDenied => "S3 access denied",
            ErrorId::SsmParameterNotFound => "SSM parameter not found",
            ErrorId::CloudFormationStackNotFound => "CloudFormation stack not found",
            
            // 4xxx - Tag Syntax & Structure Errors
            ErrorId::UnknownPreprocessingTag => "Unknown preprocessing tag",
            ErrorId::MissingRequiredTagField => "Missing required tag field",
            ErrorId::InvalidTagFieldValue => "Invalid tag field value",
            ErrorId::IncompatibleTagCombination => "Incompatible tag combination",
            ErrorId::TagSyntaxError => "Tag syntax error",
            
            // 5xxx - Type & Validation Errors
            ErrorId::TypeMismatchInOperation => "Type mismatch in operation",
            ErrorId::InvalidArrayOperation => "Invalid array operation",
            ErrorId::InvalidObjectOperation => "Invalid object operation", 
            ErrorId::DivisionByZero => "Division by zero",
            ErrorId::InvalidComparison => "Invalid comparison",
            ErrorId::StringOperationOnNonString => "String operation on non-string",
            
            // 6xxx - Template & Handlebars Errors
            ErrorId::HandlebarsSyntaxError => "Handlebars syntax error",
            ErrorId::UnknownHandlebarsHelper => "Unknown handlebars helper",
            ErrorId::HandlebarsHelperArgumentError => "Handlebars helper argument error",
            ErrorId::TemplateCompilationFailure => "Template compilation failure",
            ErrorId::TemplateExecutionError => "Template execution error",
            
            // 7xxx - CloudFormation Specific
            ErrorId::InvalidCloudFormationIntrinsic => "Invalid CloudFormation intrinsic function",
            ErrorId::CloudFormationReferenceError => "CloudFormation reference error",
            ErrorId::CloudFormationDependencyIssue => "CloudFormation dependency issue",
            ErrorId::CloudFormationTemplateSizeLimit => "CloudFormation template size limit",
            
            // 8xxx - Configuration & Setup
            ErrorId::InvalidCommandLineArgument => "Invalid command line argument",
            ErrorId::MissingRequiredConfiguration => "Missing required configuration",
            ErrorId::ConfigurationFileNotFound => "Configuration file not found",
            ErrorId::AwsCredentialsNotConfigured => "AWS credentials not configured",
            ErrorId::UnsupportedFileFormat => "Unsupported file format",
            
            // 9xxx - Internal & System Errors
            ErrorId::InternalProcessingError => "Internal processing error",
            ErrorId::MemoryAllocationFailure => "Memory allocation failure",
            ErrorId::FileSystemPermissionDenied => "File system permission denied", 
            ErrorId::NetworkConnectivityIssue => "Network connectivity issue",
            ErrorId::UnexpectedSystemError => "Unexpected system error",
        }
    }
    
    /// Get detailed explanation for CLI help
    pub fn detailed_explanation(&self) -> &'static str {
        match self {
            ErrorId::VariableNotFound => include_str!("../docs/errors/IY2001.md"),
            ErrorId::TypeMismatchInOperation => include_str!("../docs/errors/IY5001.md"),
            ErrorId::MissingRequiredTagField => include_str!("../docs/errors/IY4002.md"),
            // For now, provide basic explanation for others
            _ => "Detailed explanation not yet available. See error message for context."
        }
    }
    
    /// Parse error code string to ErrorId
    pub fn from_code(code: &str) -> Option<ErrorId> {
        let code = code.to_uppercase();
        let code = code.strip_prefix("IY").unwrap_or(&code);
        
        match code.parse::<u16>().ok()? {
            1001 => Some(ErrorId::InvalidYamlSyntax),
            1002 => Some(ErrorId::YamlVersionMismatch),
            1003 => Some(ErrorId::UnsupportedYamlFeature),
            1004 => Some(ErrorId::MalformedYamlStructure),
            1005 => Some(ErrorId::YamlMergeKeyUsage),
            
            2001 => Some(ErrorId::VariableNotFound),
            2002 => Some(ErrorId::VariableNameCollision),
            2003 => Some(ErrorId::InvalidVariableName),
            2004 => Some(ErrorId::CircularVariableReference),
            2005 => Some(ErrorId::VariableOutOfScope),
            
            3001 => Some(ErrorId::ImportFileNotFound),
            3002 => Some(ErrorId::ImportUrlUnreachable),
            3003 => Some(ErrorId::ImportAuthenticationFailure),
            3004 => Some(ErrorId::ImportCircularDependency),
            3005 => Some(ErrorId::ImportFormatNotSupported),
            3006 => Some(ErrorId::EnvironmentVariableNotFound),
            3007 => Some(ErrorId::GitCommandFailure),
            3008 => Some(ErrorId::S3AccessDenied),
            3009 => Some(ErrorId::SsmParameterNotFound),
            3010 => Some(ErrorId::CloudFormationStackNotFound),
            
            4001 => Some(ErrorId::UnknownPreprocessingTag),
            4002 => Some(ErrorId::MissingRequiredTagField),
            4003 => Some(ErrorId::InvalidTagFieldValue),
            4004 => Some(ErrorId::IncompatibleTagCombination),
            4005 => Some(ErrorId::TagSyntaxError),
            
            5001 => Some(ErrorId::TypeMismatchInOperation),
            5002 => Some(ErrorId::InvalidArrayOperation),
            5003 => Some(ErrorId::InvalidObjectOperation),
            5004 => Some(ErrorId::DivisionByZero),
            5005 => Some(ErrorId::InvalidComparison),
            5006 => Some(ErrorId::StringOperationOnNonString),
            
            6001 => Some(ErrorId::HandlebarsSyntaxError),
            6002 => Some(ErrorId::UnknownHandlebarsHelper),
            6003 => Some(ErrorId::HandlebarsHelperArgumentError),
            6004 => Some(ErrorId::TemplateCompilationFailure),
            6005 => Some(ErrorId::TemplateExecutionError),
            
            7001 => Some(ErrorId::InvalidCloudFormationIntrinsic),
            7002 => Some(ErrorId::CloudFormationReferenceError),
            7003 => Some(ErrorId::CloudFormationDependencyIssue),
            7004 => Some(ErrorId::CloudFormationTemplateSizeLimit),
            
            8001 => Some(ErrorId::InvalidCommandLineArgument),
            8002 => Some(ErrorId::MissingRequiredConfiguration),
            8003 => Some(ErrorId::ConfigurationFileNotFound),
            8004 => Some(ErrorId::AwsCredentialsNotConfigured),
            8005 => Some(ErrorId::UnsupportedFileFormat),
            
            9001 => Some(ErrorId::InternalProcessingError),
            9002 => Some(ErrorId::MemoryAllocationFailure),
            9003 => Some(ErrorId::FileSystemPermissionDenied),
            9004 => Some(ErrorId::NetworkConnectivityIssue),
            9005 => Some(ErrorId::UnexpectedSystemError),
            
            _ => None
        }
    }
    
    /// Get all error IDs for a category
    pub fn in_category(category_num: u16) -> Vec<ErrorId> {
        let start = category_num * 1000;
        let end = start + 999;
        
        let all_errors = [
            ErrorId::InvalidYamlSyntax, ErrorId::YamlVersionMismatch, ErrorId::UnsupportedYamlFeature,
            ErrorId::MalformedYamlStructure, ErrorId::YamlMergeKeyUsage,
            ErrorId::VariableNotFound, ErrorId::VariableNameCollision, ErrorId::InvalidVariableName,
            ErrorId::CircularVariableReference, ErrorId::VariableOutOfScope,
            ErrorId::ImportFileNotFound, ErrorId::ImportUrlUnreachable, ErrorId::ImportAuthenticationFailure,
            ErrorId::ImportCircularDependency, ErrorId::ImportFormatNotSupported, ErrorId::EnvironmentVariableNotFound,
            ErrorId::GitCommandFailure, ErrorId::S3AccessDenied, ErrorId::SsmParameterNotFound,
            ErrorId::CloudFormationStackNotFound,
            ErrorId::UnknownPreprocessingTag, ErrorId::MissingRequiredTagField, ErrorId::InvalidTagFieldValue,
            ErrorId::IncompatibleTagCombination, ErrorId::TagSyntaxError,
            ErrorId::TypeMismatchInOperation, ErrorId::InvalidArrayOperation, ErrorId::InvalidObjectOperation,
            ErrorId::DivisionByZero, ErrorId::InvalidComparison, ErrorId::StringOperationOnNonString,
            ErrorId::HandlebarsSyntaxError, ErrorId::UnknownHandlebarsHelper, ErrorId::HandlebarsHelperArgumentError,
            ErrorId::TemplateCompilationFailure, ErrorId::TemplateExecutionError,
            ErrorId::InvalidCloudFormationIntrinsic, ErrorId::CloudFormationReferenceError, 
            ErrorId::CloudFormationDependencyIssue, ErrorId::CloudFormationTemplateSizeLimit,
            ErrorId::InvalidCommandLineArgument, ErrorId::MissingRequiredConfiguration, 
            ErrorId::ConfigurationFileNotFound, ErrorId::AwsCredentialsNotConfigured, ErrorId::UnsupportedFileFormat,
            ErrorId::InternalProcessingError, ErrorId::MemoryAllocationFailure, ErrorId::FileSystemPermissionDenied,
            ErrorId::NetworkConnectivityIssue, ErrorId::UnexpectedSystemError,
        ];
        
        all_errors.into_iter()
            .filter(|&error_id| {
                let code = error_id as u16;
                code >= start && code <= end
            })
            .collect()
    }
    
    /// Get a detailed explanation for the error suitable for CLI help
    pub fn explain(&self) -> String {
        // Try to load from embedded documentation files first
        match self {
            ErrorId::VariableNotFound => self.detailed_explanation().to_string(),
            ErrorId::TypeMismatchInOperation => self.detailed_explanation().to_string(),
            ErrorId::MissingRequiredTagField => self.detailed_explanation().to_string(),
            _ => {
                // Fallback to generic explanation
                format!(
                    "Error {}: {}\n\n\
                    Category: {}\n\n\
                    {}\n\n\
                    This error code doesn't have detailed documentation yet.\n\
                    For more information, see the online documentation.",
                    self.code(),
                    self.description(),
                    self.category(),
                    self.description()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_id_code_format() {
        assert_eq!(ErrorId::VariableNotFound.code(), "IY2001");
        assert_eq!(ErrorId::ImportFileNotFound.code(), "IY3001");
        assert_eq!(ErrorId::MissingRequiredTagField.code(), "IY4002");
    }
    
    #[test]
    fn test_error_id_categories() {
        assert_eq!(ErrorId::VariableNotFound.category(), "Variable & Scope");
        assert_eq!(ErrorId::ImportFileNotFound.category(), "Import & Loading");
        assert_eq!(ErrorId::HandlebarsSyntaxError.category(), "Template & Handlebars");
    }
    
    #[test]
    fn test_error_id_from_code() {
        assert_eq!(ErrorId::from_code("IY2001"), Some(ErrorId::VariableNotFound));
        assert_eq!(ErrorId::from_code("iy2001"), Some(ErrorId::VariableNotFound));
        assert_eq!(ErrorId::from_code("2001"), Some(ErrorId::VariableNotFound));
        assert_eq!(ErrorId::from_code("9999"), None);
    }
    
    #[test]
    fn test_category_filtering() {
        let variable_errors = ErrorId::in_category(2);
        assert!(variable_errors.contains(&ErrorId::VariableNotFound));
        assert!(variable_errors.contains(&ErrorId::VariableNameCollision));
        assert!(!variable_errors.contains(&ErrorId::ImportFileNotFound));
    }
}