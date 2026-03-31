pub mod executor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RalfConfig {
    pub max_retries: usize,
    pub max_identical_errors: usize,
}

impl Default for RalfConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            max_identical_errors: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RalfState {
    pub retry_count: usize,
    pub error_history: Vec<String>,
}

#[cfg(test)]
mod tests;
