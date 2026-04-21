pub mod cli;
pub mod multi;
#[cfg(feature = "notification")]
pub mod notification;
#[cfg(feature = "voice")]
pub mod voice;
#[cfg(feature = "telegram")]
pub(crate) mod telegram_supervisor;

// Optional Channels
#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(test)]
mod tests;
