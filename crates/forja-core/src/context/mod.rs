use crate::error::Result;
use crate::types::Message;
use std::future::Future;
use std::pin::Pin;

pub mod compressor;
pub mod token_counter;
pub mod window;

pub type SummaryFuture = Pin<Box<dyn Future<Output = Result<String>> + Send>>;
pub type SummaryCallback = Box<dyn Fn(Vec<Message>) -> SummaryFuture + Send + Sync>;

#[cfg(test)]
mod tests;
