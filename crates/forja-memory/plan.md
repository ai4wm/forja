# forja-memory 구현 계획

## 1. 개요
`forja-memory`는 Forja 엔진의 장기 기억 장치 역할을 하는 모듈로, Phase 2의 동등성 확보(Parity) 스펙에 맞게 가볍고 추적하기 쉬운 **마크다운 파일 기반 구조**와 빠르고 정확한 **BM25 텍스트 알고리즘 검색**만으로 구현합니다. (벡터 색인 구조는 Phase 3으로 제외)

---

## 2. 파일 스토리지 설계 (Markdown 저장 구조)

DB를 사용하지 않고 파일시스템을 직접 메모리로 활용합니다. 마크다운(.md) 형태로 저장하므로 개발자 및 사용자가 직접 텍스트 에디터로 열어서 조작 및 디버깅을 하기가 매우 직관적입니다.

### 디렉토리 구조 및 파일명 규칙
```text
~/.forja/memory/
├── sessions/
│   ├── chat_1740985200_a1b2.md       # 특정 대화 세션의 컨텍스트 원본 기록
│   └── chat_1740986500_c3d4.md
└── fragments/
    ├── mem_1740985300_tag1.md        # 아카이빙된 메모리 조각 1
    └── mem_1740988800_tag2.md        # 아카이빙된 메모리 조각 2
```

### 마크다운(.md) 파일 내부 구조 패턴 (YAML Frontmatter + Body)
```markdown
---
id: mem_1740985300_tag1
timestamp: 1740985300
tags: ["project", "rust", "forja"]
---
Forja 프로젝트의 핵심 철학은 가벼운 코어와 모듈 확장성이다.
Phase 2에서는 BM25를 이용해 벡터 없이 메모리 검색 속도와 정확도를 높인다.
```

---

## 3. 핵심 인터페이스 연동 (`forja-core`)

`forja-core` 내의 리소스(`MemoryEntry`, `MemoryStore`)를 임포트하여 구현제로 사용합니다.

### 1) MemoryEntry 데이터 구조
`forja_core::types::MemoryEntry`를 직접 사용합니다.
- `id`: 고유 식별자
- `timestamp`: 저장 시각
- `tags`: 키워드 태그 속성 (문서 분류/필터링 용도)
- `content`: 내용 (MD 본문)
- `score`: 검색 정확도
- `metadata`: 직렬화 메타 정보

### 2) MemoryStore Trait 구현 방식 (save, search, flush)
```rust
use async_trait::async_trait;
use forja_core::traits::MemoryStore;
use forja_core::types::{MemoryEntry, Result};

pub struct MarkdownMemoryStore {
    base_dir: std::path::PathBuf,
}

impl MarkdownMemoryStore {
    pub fn new(base_dir: impl Into<std::path::PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }
}

#[async_trait]
impl MemoryStore for MarkdownMemoryStore {
    /// 단일 기억(MemoryEntry)을 Markdown 형태로 파싱하여 디스크에 씁니다.
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        // 1. entry 정보를 기반으로 YAML Frontmatter 생성
        // 2. entry.content 본문을 결합 후 `.md` 파일로 base_dir 하위에 저장
    }

    /// 파일들을 메모리에 올린 후 BM25 텍스트 스코어링을 통해 최적의 기억을 추출합니다.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        // 1. 디렉토리 내 모든 .md 파일 스캔 및 MemoryEntry 매핑
        // 2. 질의어(query) 토크나이즈 및 TF-IDF / BM25 기반 점수 연산
        // 3. 점수가 가장 높은 상위 `limit`개 항목 반환
    }

    /// 오버플로우 방지를 위해 오래된 항목들을 아카이빙/삭제 하는 작업을 수행합니다.
    /// (LLM을 통한 요약 기능은 Phase 3 Engine 레벨로 위임)
    async fn flush(&self) -> Result<()> {
        // base_dir/sessions 내의 파일 중 특정 시간/크기가 초과한 파일의
        // 정리(이동 혹은 폐기) 정책만 수행합니다.
    }
}
```

---

## 4. BM25 검색 토크나이징 구현 방식

*   **방식**: **In-house implementation + `unicode-segmentation`**
*   **이유**: Forja의 전체 크기를 줄이고 불필요한 색인 관리 엔진 오버헤드를 막기 위함
*   **토크나이저(분리 모델)**: 한글 형태소 분석이나 N-gram에 의존하지 않고, **`unicode-segmentation` 크레이트의 단어 경계(word boundary)** 기능만 사용하여 문장을 토큰화합니다.
*   **실행 루틴**:
    1. 검색 시 특정 메모리 풀의 텍스트 길이를 바탕으로 평균 문서 길이(avgDL) 등 역방향빈도 세분화 계산 수행
    2. BM25 공식을 코드로 구현해 메모리에 반환 점수 대입 

---

## 5. 구현 진행 순서 (작업 Step)

1.  **프로젝트 셋업**: `Cargo.toml` 의존성 (`unicode-segmentation`, `forja-core`, `serde`, `tokio` 등) 구성
2.  **스토리지 모듈**: 디렉토리 관리 및 Markdown + YAML 읽기/쓰기 유틸리티 함수 구현
3.  **검색(토크나이저) 모듈**: `unicode-segmentation` 단어 추출 분리기 및 BM25 스코어링 연산 모듈 개발
4.  **인터페이스 통합**: `MarkdownMemoryStore`의 `save`, `search`, `flush` 함수 완성
5.  **테스트 검증**: Mock DB 생성, 검색 정확도 일치 파악 및 전체 삭제 플로우 등 Unit/Integration 테스트 수행
