pub mod cli;
pub mod multi;
#[cfg(feature = "telegram")]
pub(crate) mod telegram_supervisor;

// Optional Channels
#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(test)]
mod tests;
