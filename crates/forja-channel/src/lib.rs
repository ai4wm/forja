pub mod cli;
pub mod multi;
pub mod notify_beep;
pub mod notify_terminal;
pub mod notify_toast;
pub mod switchable;
#[cfg(feature = "tui")]
pub mod tui_channel;
#[cfg(feature = "tui")]
pub mod tui_input;
#[cfg(feature = "tui")]
pub mod tui_layout;

// Optional Channels
#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;
