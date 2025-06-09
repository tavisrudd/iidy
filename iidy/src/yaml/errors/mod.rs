//! YAML error handling module
//! 
//! Contains all error-related functionality including error IDs,
//! enhanced error reporting, and error wrapper functions.

pub mod ids;
pub mod enhanced;
pub mod wrapper;

// Re-export key types
pub use ids::ErrorId;
pub use enhanced::*;
pub use wrapper::*;