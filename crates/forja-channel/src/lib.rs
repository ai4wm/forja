pub mod cli;
pub mod multi;
pub mod notify_beep;
pub mod notify_terminal;
pub mod notify_toast;

// Optional Channels
#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;
