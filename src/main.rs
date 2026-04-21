mod bootstrap;
mod config;
mod dashboard;
mod local_models;
mod oauth;
mod provider_registry;
mod runtime;
mod tui;
#[cfg(test)]
mod test_support;

use forja_core::error::Result;
use forja_core::mode::Role as ModeRole;
use runtime::shutdown::ShutdownSignal;
use runtime::startup::{build_runtime, RuntimeOptions};
use std::io::Write;
use std::time::Duration;

fn print_banner(provider_info: &str) {
    let banner = r#"
    ╔═══════════════════════════════════════╗
    ║                                       ║
    ║     ⚒️  F O R J A                      ║
    ║     Lightweight AI Agent Engine       ║
    ║     v0.1.0                            ║
    ║                                       ║
    ╚═══════════════════════════════════════╝"#;
    println!("{banner}");
    println!("    {provider_info}\n");
}

fn parse_runtime_options(args: &[String]) -> RuntimeOptions {
    let mut force_setup = false;
    let mut new_provider = None;
    let mut new_model = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--setup" => force_setup = true,
            "--provider" => {
                if index + 1 < args.len() {
                    new_provider = Some(args[index + 1].clone());
                    index += 1;
                }
            }
            "--model" => {
                if index + 1 < args.len() {
                    new_model = Some(args[index + 1].clone());
                    index += 1;
                }
            }
            _ => {}
        }
        index += 1;
    }

    RuntimeOptions {
        force_setup,
        new_provider,
        new_model,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if tui::maybe_run_tui_view(&args)? {
        return Ok(());
    }
    if args.len() >= 3 && args[1] == "login" {
        oauth::run_login(&args[2]).await;
        std::process::exit(0);
    } else if args.len() == 2 && args[1] == "login" {
        println!("Usage: forja login <provider>");
        println!("<provider> options: openai, gemini, anthropic");
        std::process::exit(1);
    }

    let _auth_data = oauth::AuthData::load();
    if args.get(1).map(String::as_str) == Some("setup") {
        config::run_setup();
        return Ok(());
    }

    let runtime = build_runtime(parse_runtime_options(&args)).await?;
    print_banner(&runtime.provider_info);
    if let Some(file_name) = &runtime.loaded_project_file {
        println!("Loaded {file_name}");
    }
    println!(
        "Mode: {} | Think: {} | Role: {}",
        runtime.exec_mode.as_str(),
        runtime.think_level.as_str(),
        ModeRole::Auto.as_str()
    );
    println!("Assistant: {}", runtime.assistant_name);
    println!("Engine is ready. Type /models to list models, /model <name> to switch.");
    if let Some(greeting) = &runtime.displayed_greeting {
        println!();
        println!("{}: {greeting}", runtime.assistant_name);
    }
    if runtime.print_initial_prompt {
        print!("\n> ");
        std::io::stdout().flush().ok();
    }

    let shutdown_signal = ShutdownSignal::new();
    let ctrlc_shutdown_signal = shutdown_signal.clone();
    ctrlc::set_handler(move || {
        let _ = ctrlc_shutdown_signal.trigger();
    })
    .map_err(|error| forja_core::error::ForjaError::Internal(error.to_string()))?;

    let mut engine = runtime.engine;
    let dashboard_server = runtime.dashboard_server;
    let channel = runtime.channel;
    let run_result = engine.run_streaming(shutdown_signal.wait()).await;

    println!("\nShutting down...");
    engine.shutdown();
    match dashboard_server.lock() {
        Ok(mut server) => server.stop(),
        Err(error) => eprintln!("[Dashboard] stop skipped: {error}"),
    }
    channel.shutdown();
    tokio::time::sleep(Duration::from_secs(1)).await;
    run_result?;

    std::process::exit(0);
}
