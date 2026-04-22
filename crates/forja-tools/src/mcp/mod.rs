mod protocol;
mod registry;
mod server;

pub use registry::build_default_registry;
pub use server::serve_stdio;
