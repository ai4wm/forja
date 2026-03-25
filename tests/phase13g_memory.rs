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

    let contents = std::fs::read_to_string(&memory_path).unwrap();

    assert!(contents.starts_with("--- 2026-03-24 ---\n"));
    assert!(contents.contains("09:30 | user | first day message"));
    assert!(contents.contains("--- 2026-03-25 ---\n08:15 | assistant | second day message"));

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

    let contents = std::fs::read_to_string(&memory_path).unwrap();

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

    assert!(!archive_dir.exists());

    store
        .flush_and_summarize(|block: String| {
            assert!(block.contains("old detail one"));
            assert!(block.contains("old detail two"));
            "summary line 1\nsummary line 2".to_string()
        })
        .await
        .unwrap();

    let contents = std::fs::read_to_string(&memory_path).unwrap();
    let archived = std::fs::read_to_string(archive_dir.join(format!("{old_day_text}.md"))).unwrap();

    assert!(archive_dir.exists());
    assert!(contents.contains(format!("--- {old_day_text} ---\nsummary line 1\nsummary line 2").as_str()));
    assert!(!contents.contains("old detail one"));
    assert!(contents.contains(format!("--- {today_text} ---").as_str()));
    assert!(contents.contains("12:00 | user | today detail"));
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

    let before = std::fs::read_to_string(&memory_path).unwrap();

    store
        .flush_and_summarize(|_block: String| -> Result<String, std::io::Error> {
            Err(std::io::Error::other("summary failed"))
        })
        .await
        .unwrap();

    let after = std::fs::read_to_string(&memory_path).unwrap();

    assert_eq!(before, after);
    assert!(!archive_dir.join(format!("{old_day_text}.md")).exists());

    let _ = tokio::fs::remove_dir_all(&base_dir).await;
}
