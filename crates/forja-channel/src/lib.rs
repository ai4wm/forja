pub mod cli;
pub mod multi;

// Optional Channels
#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "discord")]
pub mod discord;

#[cfg(test)]
mod tests;
