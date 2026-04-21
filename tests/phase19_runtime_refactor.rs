use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn main_rs_is_under_three_hundred_lines() {
    let main_rs = repo_root().join("src").join("main.rs");
    let contents = std::fs::read_to_string(main_rs).unwrap();
    let line_count = contents.lines().count();

    assert!(
        line_count < 300,
        "expected src/main.rs to stay under 300 lines, got {line_count}"
    );
}

#[test]
fn runtime_refactor_modules_exist() {
    let runtime_dir = repo_root().join("src").join("runtime");
    let required_files = [
        "mod.rs",
        "startup.rs",
        "tools.rs",
        "slash.rs",
        "shutdown.rs",
    ];

    for file in required_files {
        let path = runtime_dir.join(file);
        assert!(path.exists(), "expected {} to exist", path.display());
    }
}

#[test]
fn runtime_startup_boot_modules_exist() {
    let runtime_dir = repo_root().join("src").join("runtime");
    let required_files = [
        "boot_channel.rs",
        "boot_config.rs",
        "boot_dashboard.rs",
        "boot_engine.rs",
        "boot_memory.rs",
        "boot_profile.rs",
        "boot_provider.rs",
    ];

    for file in required_files {
        let path = runtime_dir.join(file);
        assert!(path.exists(), "expected {} to exist", path.display());
    }
}

#[test]
fn startup_rs_is_under_three_hundred_lines() {
    let startup_rs = repo_root().join("src").join("runtime").join("startup.rs");
    let contents = std::fs::read_to_string(startup_rs).unwrap();
    let line_count = contents.lines().count();

    assert!(
        line_count < 300,
        "expected src/runtime/startup.rs to stay under 300 lines, got {line_count}"
    );
}

#[test]
fn engine_dispatcher_modules_exist() {
    let engine_dir = repo_root()
        .join("crates")
        .join("forja-core")
        .join("src")
        .join("engine");
    let required_files = [
        "request.rs",
        "slash_runtime.rs",
        "state.rs",
        "streaming.rs",
        "tool_execution.rs",
        "turn.rs",
    ];

    for file in required_files {
        let path = engine_dir.join(file);
        assert!(path.exists(), "expected {} to exist", path.display());
    }
}

#[test]
fn engine_rs_is_under_three_hundred_lines() {
    let engine_rs = repo_root()
        .join("crates")
        .join("forja-core")
        .join("src")
        .join("engine.rs");
    let contents = std::fs::read_to_string(engine_rs).unwrap();
    let line_count = contents.lines().count();

    assert!(
        line_count < 300,
        "expected crates/forja-core/src/engine.rs to stay under 300 lines, got {line_count}"
    );
}
