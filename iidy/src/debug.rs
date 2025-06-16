//! Zero-cost debug logging utilities
//!
//! This module provides debug logging that compiles to nothing in release builds
//! unless explicitly enabled with the 'debug-logging' feature flag.

/// Zero-cost debug logging macro
/// Only compiles debug statements when 'debug-logging' feature is enabled
#[cfg(feature = "debug-logging")]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        log::debug!($($arg)*);
    };
}

/// Zero-cost debug logging macro - no-op when debug-logging feature is disabled
#[cfg(not(feature = "debug-logging"))]
macro_rules! debug_log {
    ($($arg:tt)*) => {};
}

pub(crate) use debug_log;
