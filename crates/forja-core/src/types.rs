use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 도구 정의 (LLM에게 전달할 명세).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// 메시지의 발신자 역할.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

/// 메시지 본문 — 텍스트, 도구 호출, 도구 결과를 구조화된 enum으로 구분.
/// 문자열 파싱이 아닌 enum 매칭으로 도구 호출을 처리한다.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    /// 일반 텍스트 메시지.
    Text { text: String },

    /// LLM이 도구 호출을 요청할 때.
    ToolCall {
        /// 호출 ID (응답 매칭용).
        call_id: String,
        /// 도구 이름 (예: "shell", "file_read").
        tool_name: String,
        /// 구조화된 JSON 인자.
        arguments: serde_json::Value,
        /// (선택적) 모델이 도구 호출 전 추론한 내용 (최신 모델 지원).
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
    },

    /// 도구 실행 결과를 LLM에게 반환할 때.
    ToolResult {
        /// 원본 호출 ID (ToolCall.call_id와 매칭).
        call_id: String,
        /// 실행 결과 (JSON).
        result: serde_json::Value,
    },
}

/// 시스템 전체에서 흐르는 단일 메시지 단위.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: Content,
    pub timestamp: u64,
    /// 토큰 수, 모델명, 채널 라우팅 정보 등 확장 가능한 메타데이터.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// 텍스트 메시지 생성 헬퍼.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content: Content::Text { text: text.into() },
            timestamp: now(),
            metadata: HashMap::new(),
        }
    }

    /// 도구 호출 메시지 생성 헬퍼.
    pub fn tool_call(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self::tool_call_with_reasoning(call_id, tool_name, arguments, None)
    }

    /// (확장용) 추론 내용(reasoning_content)을 포함한 도구 호출 생성 헬퍼.
    pub fn tool_call_with_reasoning(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: serde_json::Value,
        reasoning_content: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Assistant,
            content: Content::ToolCall {
                call_id: call_id.into(),
                tool_name: tool_name.into(),
                arguments,
                reasoning_content,
            },
            timestamp: now(),
            metadata: HashMap::new(),
        }
    }

    /// 도구 결과 메시지 생성 헬퍼.
    pub fn tool_result(call_id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: Role::Tool,
            content: Content::ToolResult {
                call_id: call_id.into(),
                result,
            },
            timestamp: now(),
            metadata: HashMap::new(),
        }
    }

    /// metadata에 key-value 추가 (빌더 패턴).
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// 내부 컨텐츠의 대략적인 문자열 길이를 반환하는 헬퍼 메서드 (토큰 수 추정용)
    pub fn content_text_len(&self) -> usize {
        match &self.content {
            Content::Text { text } => text.len(),
            Content::ToolCall { tool_name, arguments, .. } => {
                tool_name.len() + arguments.to_string().len()
            }
            Content::ToolResult { result, .. } => result.to_string().len(),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 메모리 검색 결과 항목.
/// 점수(score)와 타임스탬프를 포함하여
/// 하이브리드 검색(벡터 + BM25 + 시간 감쇠)에서 정렬 가능.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub timestamp: u64,
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}
