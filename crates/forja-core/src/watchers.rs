use crate::events::{EventQueue, FileChangeType, SystemEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tokio::task::JoinHandle;

const MEMORY_WARNING_THRESHOLD: f32 = 85.0;
const DISK_WARNING_THRESHOLD: f32 = 90.0;

#[derive(Debug, Clone)]
pub struct WatcherConfig {
    pub watch_files: bool,
    pub watch_system: bool,
    pub watch_git: bool,
    pub idle_threshold_minutes: u64,
    pub cwd: PathBuf,
    pub interval: Duration,
}

pub struct WatcherHandles {
    handles: Vec<JoinHandle<()>>,
}

impl WatcherHandles {
    pub fn new() -> Self {
        Self { handles: Vec::new() }
    }

    pub fn push(&mut self, handle: JoinHandle<()>) {
        self.handles.push(handle);
    }

    pub fn names(&self, config: &WatcherConfig) -> Vec<String> {
        let mut names = Vec::new();
        if config.watch_files {
            names.push("file".to_string());
        }
        if config.watch_system {
            names.push("system".to_string());
        }
        if config.watch_git {
            names.push("git".to_string());
        }
        names.push("idle".to_string());
        names
    }

    pub async fn stop(self) {
        for handle in self.handles {
            let _ = handle.await;
        }
    }
}

impl Default for WatcherHandles {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FileWatcher;
pub struct SystemWatcher;
pub struct GitWatcher;
pub struct IdleWatcher;

impl FileWatcher {
    pub fn spawn(
        queue: EventQueue,
        shutdown: Arc<AtomicBool>,
        config: WatcherConfig,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut previous = snapshot_files(&config.cwd);
            while !shutdown.load(Ordering::SeqCst) {
                tokio::time::sleep(config.interval).await;
                let current = snapshot_files(&config.cwd);

                for (path, modified) in &current {
                    match previous.get(path) {
                        None => queue.push(SystemEvent::FileChanged {
                            path: path.clone(),
                            change_type: FileChangeType::Created,
                        }),
                        Some(previous_modified) if previous_modified != modified => {
                            queue.push(SystemEvent::FileChanged {
                                path: path.clone(),
                                change_type: FileChangeType::Modified,
                            });
                        }
                        _ => {}
                    }
                }

                for path in previous.keys() {
                    if !current.contains_key(path) {
                        queue.push(SystemEvent::FileChanged {
                            path: path.clone(),
                            change_type: FileChangeType::Deleted,
                        });
                    }
                }

                previous = current;
            }
        })
    }
}

impl SystemWatcher {
    pub fn spawn(
        queue: EventQueue,
        shutdown: Arc<AtomicBool>,
        config: WatcherConfig,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            while !shutdown.load(Ordering::SeqCst) {
                tokio::time::sleep(config.interval).await;

                if let Some(percent) = memory_usage_percent()
                    && percent >= MEMORY_WARNING_THRESHOLD
                {
                    queue.push(SystemEvent::HighMemoryUsage { percent });
                }

                if let Some(percent) = disk_usage_percent(&config.cwd)
                    && percent >= DISK_WARNING_THRESHOLD
                {
                    queue.push(SystemEvent::HighDiskUsage { percent });
                }
            }
        })
    }
}

impl GitWatcher {
    pub fn spawn(
        queue: EventQueue,
        shutdown: Arc<AtomicBool>,
        config: WatcherConfig,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            while !shutdown.load(Ordering::SeqCst) {
                tokio::time::sleep(config.interval).await;

                if let Some(branch) = git_conflict_branch(&config.cwd) {
                    queue.push(SystemEvent::GitConflict { branch });
                }
            }
        })
    }
}

impl IdleWatcher {
    pub fn spawn(
        queue: EventQueue,
        shutdown: Arc<AtomicBool>,
        config: WatcherConfig,
        last_user_activity: Arc<Mutex<Instant>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut last_reported_minutes = 0u64;
            while !shutdown.load(Ordering::SeqCst) {
                tokio::time::sleep(config.interval).await;
                let idle_minutes = last_user_activity
                    .lock()
                    .map(|instant| instant.elapsed().as_secs() / 60)
                    .unwrap_or(0);

                if idle_minutes >= config.idle_threshold_minutes
                    && idle_minutes != last_reported_minutes
                {
                    queue.push(SystemEvent::LongIdle {
                        minutes: idle_minutes,
                    });
                    last_reported_minutes = idle_minutes;
                }
            }
        })
    }
}

pub fn snapshot_files(root: &Path) -> HashMap<String, SystemTime> {
    let mut files = HashMap::new();
    collect_files(root, root, &mut files);
    files
}

fn collect_files(root: &Path, current: &Path, files: &mut HashMap<String, SystemTime>) {
    let ignored = [".git", "target"];
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if path.is_dir() {
            if ignored.contains(&file_name) {
                continue;
            }
            collect_files(root, &path, files);
            continue;
        }

        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
        if !matches!(extension, "rs" | "toml" | "md") {
            continue;
        }
        let modified = match entry.metadata().and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(_) => continue,
        };
        let relative = path
            .strip_prefix(root)
            .ok()
            .map(|relative| relative.display().to_string())
            .unwrap_or_else(|| path.display().to_string());
        files.insert(relative, modified);
    }
}

fn memory_usage_percent() -> Option<f32> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$os=Get-CimInstance Win32_OperatingSystem; [math]::Round((($os.TotalVisibleMemorySize - $os.FreePhysicalMemory)/$os.TotalVisibleMemorySize)*100,2)",
            ])
            .output()
            .ok()?;
        parse_percent_output(&String::from_utf8_lossy(&output.stdout))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("sh")
            .args(["-c", "free | awk '/Mem:/ {print ($3/$2)*100}'"])
            .output()
            .ok()?;
        parse_percent_output(&String::from_utf8_lossy(&output.stdout))
    }
}

fn disk_usage_percent(cwd: &Path) -> Option<f32> {
    #[cfg(target_os = "windows")]
    {
        let cwd_text = cwd.display().to_string().replace('\'', "''");
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "$drive=(Get-Item -LiteralPath '{cwd_text}').PSDrive; [math]::Round(($drive.Used/($drive.Used+$drive.Free))*100,2)"
                ),
            ])
            .output()
            .ok()?;
        parse_percent_output(&String::from_utf8_lossy(&output.stdout))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let cwd_text = cwd.display().to_string().replace('\'', "'\\''");
        let output = Command::new("sh")
            .args(["-c", &format!("df -Pk '{cwd_text}' | tail -1 | awk '{{print $5}}'")])
            .output()
            .ok()?;
        parse_percent_output(&String::from_utf8_lossy(&output.stdout))
    }
}

fn git_conflict_branch(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--branch"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    let has_conflict = lines.iter().any(|line| {
        matches!(
            *line,
            line if line.starts_with("UU ")
                || line.starts_with("AA ")
                || line.starts_with("DD ")
                || line.starts_with("AU ")
                || line.starts_with("UA ")
                || line.starts_with("DU ")
                || line.starts_with("UD ")
        )
    });
    if !has_conflict {
        return None;
    }

    lines
        .first()
        .and_then(|line| line.strip_prefix("## "))
        .map(|branch| branch.split("...").next().unwrap_or(branch).to_string())
}

fn parse_percent_output(output: &str) -> Option<f32> {
    let trimmed = output.trim().trim_end_matches('%');
    trimmed.parse::<f32>().ok()
}

pub fn watcher_names(config: &WatcherConfig) -> Vec<String> {
    let mut names = Vec::new();
    if config.watch_files {
        names.push("file".to_string());
    }
    if config.watch_system {
        names.push("system".to_string());
    }
    if config.watch_git {
        names.push("git".to_string());
    }
    names.push("idle".to_string());
    names
}

#[cfg(test)]
mod tests {
    use super::{WatcherConfig, snapshot_files, watcher_names};
    use crate::events::EventQueue;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("forja_watchers_{name}_{nanos}"))
    }

    #[test]
    fn watcher_names_reflect_enabled_watchers() {
        let config = WatcherConfig {
            watch_files: true,
            watch_system: true,
            watch_git: false,
            idle_threshold_minutes: 30,
            cwd: PathBuf::from("."),
            interval: Duration::from_secs(1),
        };

        assert_eq!(watcher_names(&config), vec!["file", "system", "idle"]);
    }

    #[test]
    fn snapshot_files_collects_supported_extensions() {
        let base_dir = unique_temp_dir("snapshot");
        std::fs::create_dir_all(&base_dir).unwrap();
        std::fs::write(base_dir.join("lib.rs"), "fn main() {}").unwrap();
        std::fs::write(base_dir.join("notes.md"), "# Notes").unwrap();
        std::fs::write(base_dir.join("ignore.txt"), "ignore").unwrap();

        let snapshot = snapshot_files(&base_dir);

        assert!(snapshot.contains_key("lib.rs"));
        assert!(snapshot.contains_key("notes.md"));
        assert!(!snapshot.contains_key("ignore.txt"));
    }

    #[tokio::test]
    async fn idle_watcher_can_push_events() {
        let queue = EventQueue::new();
        let shutdown = Arc::new(AtomicBool::new(false));
        let config = WatcherConfig {
            watch_files: false,
            watch_system: false,
            watch_git: false,
            idle_threshold_minutes: 0,
            cwd: PathBuf::from("."),
            interval: Duration::from_millis(10),
        };
        let last_user_activity = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(61)));

        let handle = super::IdleWatcher::spawn(
            queue.clone(),
            shutdown.clone(),
            config,
            last_user_activity,
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        shutdown.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.await;

        assert!(queue.len() >= 1);
    }
}
