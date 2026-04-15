use chrono::{Datelike, Duration, Local, TimeZone};
use forja_core::traits::MemoryStore;
use forja_core::types::MemoryEntry;
use forja_memory::MarkdownMemoryStore;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("forja_{name}_{nanos}"))
}

fn memory_entry(id: &str, role: &str, content: &str, timestamp: u64) -> MemoryEntry {
    MemoryEntry {
        id: id.to_string(),
        content: content.to_string(),
        score: 0.0,
        timestamp,
        tags: vec![role.to_string()],
        metadata: Default::default(),
    }
}

fn local_timestamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> u64 {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .unwrap()
        .timestamp() as u64
}

fn midday_timestamp(date: chrono::NaiveDate) -> u64 {
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 12, 0, 0)
        .single()
        .unwrap()
        .timestamp() as u64
}

#[tokio::test]
async fn save_inserts_date_headers_when_the_day_changes() {
    let base_dir = unique_temp_dir("phase13g_headers");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let first_day = "2026-03-24";
    let second_day = "2026-03-25";

    store
        .save(&memory_entry(
            "entry-1",
            "user",
            "first day message",
            local_timestamp(2026, 3, 24, 9, 30),
        ))
        .await
        .unwrap();
    store
        .save(&memory_entry(
            "entry-2",
            "assistant",
            "second day message",
            local_timestamp(2026, 3, 25, 8, 15),
        ))
        .await
        .unwrap();

    let first_contents = std::fs::read_to_string(base_dir.join("daily").join(format!("{first_day}.md")))
        .unwrap();
    let second_contents =
        std::fs::read_to_string(base_dir.join("daily").join(format!("{second_day}.md"))).unwrap();

    assert!(first_contents.contains("09:30 | user | first day message"));
    assert!(second_contents.contains("08:15 | assistant | second day message"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn save_filters_out_mockstream_lines() {
    let base_dir = unique_temp_dir("phase13g_mockstream");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();

    store
        .save(&memory_entry(
            "entry-1",
            "assistant",
            "[MockStream] should never be stored",
            local_timestamp(2026, 3, 24, 10, 0),
        ))
        .await
        .unwrap();
    store
        .save(&memory_entry(
            "entry-2",
            "user",
            "real message",
            local_timestamp(2026, 3, 24, 10, 1),
        ))
        .await
        .unwrap();

    let date = "2026-03-24";
    let contents = std::fs::read_to_string(base_dir.join("daily").join(format!("{date}.md"))).unwrap();

    assert!(!contents.contains("MockStream"));
    assert!(contents.contains("10:01 | user | real message"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn flush_and_summarize_replaces_old_blocks_and_archives_originals() {
    let base_dir = unique_temp_dir("phase13g_summarize");
    let memory_path = base_dir.join("memory.md");
    let archive_dir = base_dir.join("archive");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let today = Local::now().date_naive();
    let old_day = today - Duration::days(2);
    let old_day_text = old_day.format("%Y-%m-%d").to_string();
    let today_text = today.format("%Y-%m-%d").to_string();

    store
        .save(&memory_entry(
            "entry-1",
            "user",
            "old detail one",
            midday_timestamp(old_day),
        ))
        .await
        .unwrap();
    store
        .save(&memory_entry(
            "entry-2",
            "assistant",
            "old detail two",
            midday_timestamp(old_day) + 60,
        ))
        .await
        .unwrap();
    store
        .save(&memory_entry(
            "entry-3",
            "user",
            "today detail",
            midday_timestamp(today),
        ))
        .await
        .unwrap();

    store
        .flush_and_summarize(|block: String| {
            assert!(block.contains("old detail one"));
            assert!(block.contains("old detail two"));
            "summary line 1\nsummary line 2".to_string()
        })
        .await
        .unwrap();

    let contents =
        std::fs::read_to_string(base_dir.join("daily").join(format!("{old_day_text}.md"))).unwrap();
    let archived = std::fs::read_to_string(archive_dir.join(format!("{old_day_text}.md"))).unwrap();

    assert!(archive_dir.exists());
    assert_eq!(contents.trim(), "summary line 1\nsummary line 2");
    assert!(!contents.contains("old detail one"));
    let today_contents =
        std::fs::read_to_string(base_dir.join("daily").join(format!("{today_text}.md"))).unwrap();
    assert!(today_contents.contains("12:00 | user | today detail"));
    assert!(archived.contains("old detail one"));
    assert!(archived.contains("old detail two"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn flush_and_summarize_keeps_original_when_summarizer_fails() {
    let base_dir = unique_temp_dir("phase13g_summary_fail");
    let memory_path = base_dir.join("memory.md");
    let archive_dir = base_dir.join("archive");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let old_day = Local::now().date_naive() - Duration::days(2);
    let old_day_text = old_day.format("%Y-%m-%d").to_string();

    store
        .save(&memory_entry(
            "entry-1",
            "user",
            "keep me raw",
            midday_timestamp(old_day),
        ))
        .await
        .unwrap();

    let before =
        std::fs::read_to_string(base_dir.join("daily").join(format!("{old_day_text}.md"))).unwrap();

    store
        .flush_and_summarize(|_block: String| -> Result<String, std::io::Error> {
            Err(std::io::Error::other("summary failed"))
        })
        .await
        .unwrap();

    let after =
        std::fs::read_to_string(base_dir.join("daily").join(format!("{old_day_text}.md"))).unwrap();

    assert_eq!(before.trim(), after.trim());
    assert!(!archive_dir.join(format!("{old_day_text}.md")).exists());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn init_creates_wiki_style_memory_layout() {
    let base_dir = unique_temp_dir("phase13g_wiki_layout");
    let memory_path = base_dir.join("memory.md");

    let _store = MarkdownMemoryStore::new(&memory_path).await.unwrap();

    assert!(base_dir.join("index.md").exists());
    assert!(base_dir.join("topics").exists());
    assert!(base_dir.join("daily").exists());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn save_updates_daily_log_index_and_relevant_topic_context() {
    let base_dir = unique_temp_dir("phase13g_topic_context");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let day = "2026-03-24";

    store
        .save(&memory_entry(
            "entry-1",
            "user",
            "I prefer oolong tea after dinner.",
            local_timestamp(2026, 3, 24, 20, 15),
        ))
        .await
        .unwrap();

    let daily_log = std::fs::read_to_string(base_dir.join("daily").join(format!("{day}.md")))
        .unwrap();
    let index = std::fs::read_to_string(base_dir.join("index.md")).unwrap();
    let relevant = store.load_relevant("What tea do I like?").await.unwrap();

    assert!(daily_log.contains("I prefer oolong tea after dinner."));
    assert!(index.to_lowercase().contains("preferences"));
    assert!(index.contains("oolong tea"));
    assert!(relevant.contains("oolong tea"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn startup_context_loads_only_recent_three_daily_files() {
    let base_dir = unique_temp_dir("phase13g_startup_budget");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let today = Local::now().date_naive();
    let dates = [
        today - Duration::days(3),
        today - Duration::days(2),
        today - Duration::days(1),
        today,
    ];

    for (index, date) in dates.into_iter().enumerate() {
        store
            .save(&memory_entry(
                &format!("entry-{index}"),
                "user",
                &format!("daily note {index}"),
                midday_timestamp(date),
            ))
            .await
            .unwrap();
    }

    let startup = store.load_startup_context().await.unwrap();

    assert!(!startup.contains("daily note 0"));
    assert!(startup.contains("daily note 1"));
    assert!(startup.contains("daily note 2"));
    assert!(startup.contains("daily note 3"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn legacy_memory_md_is_auto_migrated_into_new_layout() {
    let base_dir = unique_temp_dir("phase13g_legacy_memory_md");
    let memory_path = base_dir.join("memory.md");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::write(
        &memory_path,
        "--- 2026-03-24 ---\n09:30 | user | I prefer oolong tea.\n09:31 | assistant | You like oolong tea.\n",
    )
    .unwrap();

    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let startup = store.load_startup_context().await.unwrap();
    let relevant = store.load_relevant("tea").await.unwrap();

    assert!(base_dir.join("index.md").exists());
    assert!(base_dir.join("daily").join("2026-03-24.md").exists());
    assert!(startup.contains("oolong tea"));
    assert!(relevant.contains("oolong tea"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn init_rebuilds_index_from_existing_topic_files() {
    let base_dir = unique_temp_dir("phase13g_rebuild_index");
    std::fs::create_dir_all(base_dir.join("topics")).unwrap();
    std::fs::create_dir_all(base_dir.join("daily")).unwrap();
    std::fs::write(
        base_dir.join("topics").join("preferences.md"),
        "# Topic: preferences\n- [2026-03-24 09:30] user | I prefer oolong tea.\n",
    )
    .unwrap();
    std::fs::write(base_dir.join("index.md"), "").unwrap();

    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let index = std::fs::read_to_string(base_dir.join("index.md")).unwrap();
    let relevant = store.load_relevant("tea").await.unwrap();

    assert!(index.contains("preferences"));
    assert!(index.contains("oolong tea"));
    assert!(relevant.contains("oolong tea"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}

#[tokio::test]
async fn relevant_loading_limits_the_number_of_topic_shards() {
    let base_dir = unique_temp_dir("phase13g_shard_limit");
    let memory_path = base_dir.join("memory.md");
    let store = MarkdownMemoryStore::new(&memory_path).await.unwrap();
    let timestamp = local_timestamp(2026, 3, 24, 9, 30);

    for marker in ["marker-one", "marker-two", "marker-three", "marker-four", "marker-five"] {
        let content = format!(
            "Project Atlas {marker} {}",
            "alpha ".repeat(700)
        );
        store
            .save(&memory_entry(marker, "user", &content, timestamp))
            .await
            .unwrap();
    }

    let topic_files = std::fs::read_dir(base_dir.join("topics"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("projects"))
        .collect::<Vec<_>>();
    let relevant = store.load_relevant("Project Atlas").await.unwrap();

    assert!(topic_files.len() >= 3);
    assert!(!relevant.contains("marker-one"));
    assert!(relevant.contains("marker-four") || relevant.contains("marker-five"));

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}
