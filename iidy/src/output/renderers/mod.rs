//! Renderer implementations for different output modes

pub mod plain;
pub mod interactive;
pub mod json;

pub use plain::PlainTextRenderer;
pub use interactive::InteractiveRenderer;
pub use json::JsonRenderer;