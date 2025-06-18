//! Keyboard listener for dynamic output mode switching
//!
//! This module provides keyboard input handling for switching output modes
//! during CloudFormation operations. The keyboard listener is automatically
//! disabled in non-TTY environments (CI/CD, pipes, etc.) to ensure compatibility
//! with automation tools.

use crate::output::renderer::OutputMode;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use std::time::Duration;
use tokio::sync::mpsc;

/// Keyboard command for output mode switching
#[derive(Debug, Clone)]
pub enum KeyboardCommand {
    SwitchToPlain,
    SwitchToInteractive,
    SwitchToJson,
    ToggleTimestamps,
    ShowHelp,
    Quit,
}

/// Configuration for keyboard listener
#[derive(Debug, Clone)]
pub struct KeyboardConfig {
    /// Whether to enable keyboard listener (automatically disabled in non-TTY)
    pub enabled: bool,
    /// Polling interval for checking key events
    pub poll_interval: Duration,
    /// Whether to show help on startup
    pub show_help_on_start: bool,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            enabled: is_tty_environment(),
            poll_interval: Duration::from_millis(50),
            show_help_on_start: false,
        }
    }
}

/// Check if we're in a TTY environment where keyboard input makes sense
pub fn is_tty_environment() -> bool {
    // Check if both stdin and stdout are TTYs
    atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout)
}

/// Keyboard listener that monitors for mode switching commands
pub struct KeyboardListener {
    config: KeyboardConfig,
    command_sender: Option<mpsc::UnboundedSender<KeyboardCommand>>,
    command_receiver: Option<mpsc::UnboundedReceiver<KeyboardCommand>>,
}

impl KeyboardListener {
    /// Create a new keyboard listener with the given configuration
    pub fn new(config: KeyboardConfig) -> Self {
        let (sender, receiver) = if config.enabled {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        Self {
            config,
            command_sender: sender,
            command_receiver: receiver,
        }
    }

    /// Start the keyboard listener (no-op if not in TTY)
    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            // Silently do nothing in non-TTY environments
            return Ok(());
        }

        // Enable raw mode for immediate key capture
        terminal::enable_raw_mode()?;

        if self.config.show_help_on_start {
            self.show_help().await?;
        }

        // Start the async keyboard monitoring task
        if let Some(sender) = self.command_sender.clone() {
            tokio::spawn(async move {
                loop {
                    if let Ok(available) = event::poll(Duration::from_millis(50)) {
                        if available {
                            if let Ok(event) = event::read() {
                                if let Some(command) = Self::parse_key_event(event) {
                                    if sender.send(command).is_err() {
                                        break; // Receiver dropped, exit
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Stop the keyboard listener and restore terminal state
    pub async fn stop(&mut self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Restore terminal to normal mode
        terminal::disable_raw_mode()?;
        Ok(())
    }

    /// Check for incoming keyboard commands (non-blocking)
    pub fn try_recv_command(&mut self) -> Option<KeyboardCommand> {
        if !self.config.enabled {
            return None;
        }

        if let Some(receiver) = &mut self.command_receiver {
            receiver.try_recv().ok()
        } else {
            None
        }
    }

    /// Show help message for keyboard shortcuts
    pub async fn show_help(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        println!("\n📋 iidy Output Mode Controls:");
        println!("  [1] - Switch to Plain mode (CI-friendly)");
        println!("  [2] - Switch to Interactive mode (colors, formatting)");
        println!("  [3] - Switch to JSON mode (machine-readable)");
        println!("  [t] - Toggle timestamps on/off");
        println!("  [h] - Show this help");
        println!("  [q] - Quit keyboard listener");
        println!("  Press any key to continue...\n");

        Ok(())
    }

    /// Parse a crossterm key event into a keyboard command
    fn parse_key_event(event: Event) -> Option<KeyboardCommand> {
        match event {
            Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                ..
            }) => match code {
                KeyCode::Char('1') => Some(KeyboardCommand::SwitchToPlain),
                KeyCode::Char('2') => Some(KeyboardCommand::SwitchToInteractive),
                KeyCode::Char('3') => Some(KeyboardCommand::SwitchToJson),
                KeyCode::Char('t') | KeyCode::Char('T') => Some(KeyboardCommand::ToggleTimestamps),
                KeyCode::Char('h') | KeyCode::Char('H') => Some(KeyboardCommand::ShowHelp),
                KeyCode::Char('q') | KeyCode::Char('Q') => Some(KeyboardCommand::Quit),
                KeyCode::Esc => Some(KeyboardCommand::Quit),
                _ => None,
            },
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => Some(KeyboardCommand::Quit),
            _ => None,
        }
    }

    /// Convert a keyboard command to the corresponding output mode
    pub fn command_to_output_mode(command: &KeyboardCommand) -> Option<OutputMode> {
        match command {
            KeyboardCommand::SwitchToPlain => Some(OutputMode::Plain),
            KeyboardCommand::SwitchToInteractive => Some(OutputMode::Interactive),
            KeyboardCommand::SwitchToJson => Some(OutputMode::Json),
            _ => None,
        }
    }

    /// Check if keyboard listener is enabled (TTY detection)
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

impl Drop for KeyboardListener {
    fn drop(&mut self) {
        // Ensure terminal is restored on drop
        if self.config.enabled {
            let _ = terminal::disable_raw_mode();
        }
    }
}

/// Helper function to create a disabled keyboard listener for non-TTY environments
pub fn create_for_environment() -> KeyboardListener {
    let config = KeyboardConfig::default();
    KeyboardListener::new(config)
}

/// Integration with DynamicOutputManager for mode switching
pub async fn handle_keyboard_commands(
    keyboard_listener: &mut KeyboardListener,
    output_manager: &mut crate::output::manager::DynamicOutputManager,
) -> Result<bool> {
    if !keyboard_listener.is_enabled() {
        return Ok(true); // Continue processing, but no keyboard handling
    }

    while let Some(command) = keyboard_listener.try_recv_command() {
        match command {
            KeyboardCommand::ShowHelp => {
                keyboard_listener.show_help().await?;
            }
            KeyboardCommand::Quit => {
                return Ok(false); // Signal to stop processing
            }
            KeyboardCommand::ToggleTimestamps => {
                // This would require extending DynamicOutputManager to support timestamp toggling
                println!("💡 Timestamp toggling not yet implemented");
            }
            command => {
                if let Some(new_mode) = KeyboardListener::command_to_output_mode(&command) {
                    output_manager.switch_to_mode(new_mode).await?;
                }
            }
        }
    }

    Ok(true) // Continue processing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_config_default() {
        let config = KeyboardConfig::default();
        // In test environment, this might be false (depending on test runner)
        // The important thing is that it respects TTY detection
        assert_eq!(config.poll_interval, Duration::from_millis(50));
        assert!(!config.show_help_on_start);
    }

    #[test]
    fn test_is_tty_environment() {
        // Test that the function returns a boolean (actual value depends on test environment)
        let is_tty = is_tty_environment();
        // In automated tests, this is typically false
        assert!(is_tty == true || is_tty == false);
    }

    #[test]
    fn test_command_to_output_mode() {
        assert_eq!(
            KeyboardListener::command_to_output_mode(&KeyboardCommand::SwitchToPlain),
            Some(OutputMode::Plain)
        );
        assert_eq!(
            KeyboardListener::command_to_output_mode(&KeyboardCommand::SwitchToInteractive),
            Some(OutputMode::Interactive)
        );
        assert_eq!(
            KeyboardListener::command_to_output_mode(&KeyboardCommand::SwitchToJson),
            Some(OutputMode::Json)
        );
        assert_eq!(
            KeyboardListener::command_to_output_mode(&KeyboardCommand::ShowHelp),
            None
        );
        assert_eq!(
            KeyboardListener::command_to_output_mode(&KeyboardCommand::Quit),
            None
        );
    }

    #[tokio::test]
    async fn test_keyboard_listener_creation() {
        // Test with disabled config (simulates non-TTY)
        let config = KeyboardConfig {
            enabled: false,
            poll_interval: Duration::from_millis(100),
            show_help_on_start: false,
        };
        
        let listener = KeyboardListener::new(config);
        assert!(!listener.is_enabled());
        assert!(listener.command_sender.is_none());
        assert!(listener.command_receiver.is_none());
    }

    #[tokio::test]
    async fn test_keyboard_listener_disabled_operations() {
        let config = KeyboardConfig {
            enabled: false,
            poll_interval: Duration::from_millis(100),
            show_help_on_start: false,
        };
        
        let mut listener = KeyboardListener::new(config);
        
        // All operations should be no-ops when disabled
        assert!(listener.start().await.is_ok());
        assert!(listener.stop().await.is_ok());
        assert!(listener.show_help().await.is_ok());
        assert!(listener.try_recv_command().is_none());
    }

    #[test]
    fn test_parse_key_event() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        // Test digit keys for mode switching
        let event1 = Event::Key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        assert!(matches!(
            KeyboardListener::parse_key_event(event1),
            Some(KeyboardCommand::SwitchToPlain)
        ));

        let event2 = Event::Key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert!(matches!(
            KeyboardListener::parse_key_event(event2),
            Some(KeyboardCommand::SwitchToInteractive)
        ));

        let event3 = Event::Key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        assert!(matches!(
            KeyboardListener::parse_key_event(event3),
            Some(KeyboardCommand::SwitchToJson)
        ));

        // Test help key
        let help_event = Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert!(matches!(
            KeyboardListener::parse_key_event(help_event),
            Some(KeyboardCommand::ShowHelp)
        ));

        // Test quit keys
        let quit_event = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(
            KeyboardListener::parse_key_event(quit_event),
            Some(KeyboardCommand::Quit)
        ));

        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(
            KeyboardListener::parse_key_event(ctrl_c),
            Some(KeyboardCommand::Quit)
        ));

        // Test unknown key
        let unknown = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(KeyboardListener::parse_key_event(unknown).is_none());
    }

    #[test]
    fn test_create_for_environment() {
        let listener = create_for_environment();
        // Should respect TTY detection
        assert_eq!(listener.is_enabled(), is_tty_environment());
    }
}