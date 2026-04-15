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
