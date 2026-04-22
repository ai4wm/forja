pub mod cli;
pub mod dashboard_bridge;
pub mod multi;
#[cfg(feature = "notification")]
pub mod notification;
#[cfg(feature = "telegram")]
pub(crate) mod telegram_supervisor;
#[cfg(feature = "voice")]
pub mod voice;

// Optional Channels
#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(test)]
mod tests;
