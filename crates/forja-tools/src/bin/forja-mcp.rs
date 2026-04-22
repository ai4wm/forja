use forja_tools::mcp;

#[tokio::main]
async fn main() {
    if let Err(error) = mcp::serve_stdio().await {
        eprintln!("forja-mcp error: {error}");
        std::process::exit(1);
    }
}
