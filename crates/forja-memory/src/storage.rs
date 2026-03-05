use forja_core::error::{ForjaError as Error, Result};
use forja_core::types::MemoryEntry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;

/// YAML Frontmatter 구조체. MemoryEntry 로 변환/직렬화하기 위한 용도.
#[derive(Debug, Serialize, Deserialize)]
struct Frontmatter {
    id: String,
    timestamp: u64,
    tags: Vec<String>,
}

/// 파일 시스템 기반의 마크다운 저장소 관리 모듈
#[derive(Debug, Clone)]
pub struct Storage {
    pub base_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub fragments_dir: PathBuf,
}

impl Storage {
    /// 기본 디렉토리 구조를 생성하고 Storage 인스턴스를 반환합니다.
    pub async fn init(base_dir: impl AsRef<Path>) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        let sessions_dir = base_dir.join("sessions");
        let fragments_dir = base_dir.join("fragments");

        // 디렉토리 트리 생성
        fs::create_dir_all(&sessions_dir)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create sessions dir: {}", e)))?;
        fs::create_dir_all(&fragments_dir)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create fragments dir: {}", e)))?;

        Ok(Self {
            base_dir,
            sessions_dir,
            fragments_dir,
        })
    }

    /// MemoryEntry 객체를 Markdown 파일로 저장합니다.
    /// 파일 위치: base_dir/sessions/{id}.md
    pub async fn write_entry(&self, entry: &MemoryEntry) -> Result<()> {
        let frontmatter = Frontmatter {
            id: entry.id.clone(),
            timestamp: entry.timestamp,
            tags: entry.tags.clone(),
        };

        // YAML 직렬화
        let yaml_str = serde_yaml::to_string(&frontmatter)
            .map_err(|e| Error::Serialization(format!("YAML serialize error: {}", e)))?;

        // Markdown 형식 생성 (---\n YAML \n---\n 본문)
        let md_content = format!("---\n{}---\n{}", yaml_str, entry.content);

        // 파일 쓰기
        let file_path = self.sessions_dir.join(format!("{}.md", entry.id));
        fs::write(&file_path, md_content)
            .await
            .map_err(|e| Error::Storage(format!("Failed to write memory file: {}", e)))?;

        Ok(())
    }

    /// 디렉토리 내의 모든 마크다운 파일을 읽어 MemoryEntry 리스트로 파싱합니다.
    pub async fn read_all_entries(&self) -> Result<Vec<MemoryEntry>> {
        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(&self.sessions_dir)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read sessions dir: {}", e)))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(memory_entry) = Self::parse_file(&path).await {
                    entries.push(memory_entry);
                } else {
                    // 오류 파일은 로깅 혹은 무시 (초경량 엔진에서는 일단 무시)
                }
            }
        }

        // fragments_dir 하위에 아카이브 된 항목도 로딩 (검색 범위 포함)
        let mut fragment_dir = fs::read_dir(&self.fragments_dir)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read fragments dir: {}", e)))?;

        while let Some(entry) = fragment_dir
            .next_entry()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Ok(memory_entry) = Self::parse_file(&path).await {
                    entries.push(memory_entry);
                }
        }

        Ok(entries)
    }

    /// 단일 `.md` 파일을 읽어 `MemoryEntry`로 변환합니다.
    async fn parse_file(path: &Path) -> Result<MemoryEntry> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read file {:?}: {}", path, e)))?;

        // `---` 경계선(yaml block) 분리 로직
        // 빈 파일이거나 형식 불일치 대비
        if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
            return Err(Error::Storage(format!("Invalid Frontmatter format in {:?}", path)));
        }

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        // 배열 길이는 빈 요소 포함 보통 3개여야 함 ("" / yaml_data / content_body)
        if parts.len() < 3 {
            return Err(Error::Storage(format!("Cannot parse Markdown YAML block in {:?}", path)));
        }

        let yaml_section = parts[1].trim();
        let md_body = parts[2].trim_start(); // ---\n 바로 뒤부터가 본문

        let fm: Frontmatter = serde_yaml::from_str(yaml_section)
            .map_err(|e| Error::Deserialization(format!("YAML deserialize error: {}", e)))?;

        Ok(MemoryEntry {
            id: fm.id,
            timestamp: fm.timestamp,
            tags: fm.tags,
            content: md_body.to_string(),
            score: 0.0, // 검색엔진 처리 전 기본값
            metadata: Default::default(),
        })
    }

    /// 오래된 세션들을 `sessions` 디렉토리에서 `fragments`(아카이브)로 이동시킵니다. (오래된 순)
    pub async fn archive_old_files(&self, retain_count: usize) -> Result<()> {
        let mut session_files = Vec::new();
        let mut read_dir = fs::read_dir(&self.sessions_dir)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read sessions dir: {}", e)))?;

        // 파일 수집 및 최근 수정 시간 추출
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| Error::Storage(e.to_string()))?
        {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Ok(meta) = fs::metadata(&path).await {
                    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    session_files.push((path, mtime));
                }
        }

        // 최근 파일순 정렬 (최신이 앞에 오도록 수정 시간에 대한 Reverse 처리)
        // SystemTime 비교
        session_files.sort_by(|a, b| b.1.cmp(&a.1));

        // retain_count를 초과하는 파일들은 모두 fragments_dir 로 이동
        if session_files.len() > retain_count {
            for (path, _) in session_files.into_iter().skip(retain_count) {
                if let Some(file_name) = path.file_name() {
                    let dest = self.fragments_dir.join(file_name);
                    fs::rename(&path, &dest).await.map_err(|e| {
                        Error::Storage(format!("Failed to move file to fragments: {}", e))
                    })?;
                }
            }
        }

        Ok(())
    }
}
