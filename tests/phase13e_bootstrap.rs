use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_{name}_{nanos}"))
}

fn spawn_forja(home_dir: &Path) -> std::process::Child {
    std::fs::create_dir_all(home_dir).unwrap();
    let forja_bin = PathBuf::from(env!("CARGO_BIN_EXE_forja"));
    let forja_bin = if forja_bin.is_absolute() {
        forja_bin
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(forja_bin)
    };

    Command::new(forja_bin)
        .current_dir(home_dir)
        .env("FORJA_USE_MOCK", "1")
        .env("FORJA_PROVIDER", "ollama")
        .env("FORJA_MODEL", "qwen3.5:9b")
        .env("FORJA_HOME_DIR", home_dir)
        .env("USERPROFILE", home_dir)
        .env("HOME", home_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn kill_and_collect_output(mut child: std::process::Child) -> Output {
    let _ = child.kill();
    child.wait_with_output().unwrap()
}

fn wait_for_path(path: &Path, timeout: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    panic!("timed out waiting for {}", path.display());
}

#[test]
fn first_run_without_identity_starts_bootstrap_onboarding() {
    let home_dir = unique_temp_dir("phase13e_first_run");
    let child = spawn_forja(&home_dir);

    std::thread::sleep(Duration::from_millis(800));
    let output = kill_and_collect_output(child);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("What should I call myself?"));

    let _ = std::fs::remove_dir_all(&home_dir);
}

#[test]
fn completed_onboarding_persists_identity_and_skips_questions_on_restart() {
    let home_dir = unique_temp_dir("phase13e_restart");
    let mut first_child = spawn_forja(&home_dir);

    first_child
        .stdin
        .as_mut()
        .unwrap()
        .write_all("황비서\n주인님\n존댓말\n개인 비서\n".as_bytes())
        .unwrap();
    first_child.stdin.as_mut().unwrap().flush().unwrap();

    let identity_path = home_dir.join(".forja").join("identity.md");
    let user_path = home_dir.join(".forja").join("user.md");
    wait_for_path(&identity_path, Duration::from_secs(5));
    wait_for_path(&user_path, Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(1500));

    let first_output = kill_and_collect_output(first_child);
    let first_stdout = String::from_utf8_lossy(&first_output.stdout);
    let identity = std::fs::read_to_string(&identity_path).unwrap();
    let user = std::fs::read_to_string(&user_path).unwrap();

    assert!(identity.contains("name: 황비서"));
    assert!(identity.contains("tone: 존댓말"));
    assert!(identity.contains("role: 개인 비서"));
    assert!(user.contains("name: 주인님"));
    assert!(!first_stdout.contains("Hello, 주인님! I am 황비서. How can I help?"));

    let second_child = spawn_forja(&home_dir);
    std::thread::sleep(Duration::from_millis(800));
    let second_output = kill_and_collect_output(second_child);
    let second_stdout = String::from_utf8_lossy(&second_output.stdout);

    assert!(!second_stdout.contains("What should I call myself?"));
    assert!(!second_stdout.contains("How should I address you?"));

    let _ = std::fs::remove_dir_all(&home_dir);
}

#[test]
fn startup_with_existing_identity_skips_bootstrap_onboarding() {
    let home_dir = unique_temp_dir("phase13e_existing_identity");
    let forja_dir = home_dir.join(".forja");
    std::fs::create_dir_all(&forja_dir).unwrap();
    std::fs::write(
        forja_dir.join("identity.md"),
        "---\nname: 황비서\nrole: 개인 비서\ntone: 존댓말\n---\n",
    )
    .unwrap();
    std::fs::write(forja_dir.join("user.md"), "---\nname: 주인님\n---\n").unwrap();

    let child = spawn_forja(&home_dir);
    std::thread::sleep(Duration::from_millis(800));
    let output = kill_and_collect_output(child);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!stdout.contains("What should I call myself?"));
    assert!(!stdout.contains("How should I address you?"));

    let _ = std::fs::remove_dir_all(&home_dir);
}
