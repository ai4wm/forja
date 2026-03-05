# Config 다중 프로바이더 Key 관리 개선 기획안

## 1. 개요

현재 `[llm]` 섹션에 단일 프로바이더만 저장하는 구조를 **`[keys]` + `[active]` 분리 구조**로 확장합니다.
모든 프로바이더 API 키를 한번에 저장해 두고, `[active]`만 바꿔 모델을 전환하는 방식입니다.
환경변수와 CLI 플래그가 우선 적용됩니다.

---

## 2. config.toml 신규 포맷

```toml
[active]
provider = "moonshot"
model    = "kimi-k2.5"   # 생략 시 preset 기본값

[keys]
openai    = "sk-xxx"
anthropic = "sk-ant-xxx"
gemini    = "AIza-xxx"
deepseek  = "sk-ds-xxx"
glm       = "glm-xxx"
moonshot  = "sk-zR8H94-xxx"
# ollama는 키 불필요, 생략
```

## 3. 로드 우선순위 (높은 순)

1. **CLI 플래그** (`--provider`, `--model`, `--setup`)
2. **환경변수** (`FORJA_PROVIDER`, `FORJA_API_KEY`, `FORJA_MODEL`)
3. **`~/.forja/config.toml`** 파일
4. **대화형 온보딩** (파일도 없고, 환경변수도 없을 때만)

---

## 4. 대화형 온보딩 개선 (여러 키 한번에 입력)

```
🔧 Forja 첫 실행 감지. 설정을 시작합니다.

각 프로바이더의 API 키를 입력하세요. 없으면 Enter 스킵.

OpenAI API 키     > sk-xxx
Anthropic API 키  > (Enter 스킵)
Gemini API 키     > (Enter 스킵)
DeepSeek API 키   > (Enter 스킵)
GLM API 키        > (Enter 스킵)
Moonshot API 키   > sk-zR8H94-xxx

기본 프로바이더 선택 (저장된 키: openai, moonshot):
  1. openai (gpt-5.2)
  5. moonshot (kimi-k2.5)
번호 입력 > 5

✅ ~/.forja/config.toml 저장 완료.
```

---

## 5. CLI 플래그 설계 (std::env::args() 파싱)

별도의 라이브러리 없이 `std::env::args()`를 순회하여 간단히 파싱합니다.

```
forja [FLAGS]

FLAGS:
  --setup             전체 재설정 (온보딩 재실행 + 파일 덮어쓰기)
  --provider <name>   프로바이더 전환 (저장된 키 사용, 없으면 키 입력 요청)
  --model <name>      모델명만 변경 (현재 프로바이더 유지)
```

저장된 키가 없는 프로바이더 전환 시:
```
moonshot 키가 없습니다. Moonshot API 키를 입력하세요 > sk-...
키를 config.toml에 저장했습니다.
```

---

## 6. 구조체 변경 (`config.rs`)

```rust
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ForjaConfig {
    pub active: ActiveSection,
    pub keys:   KeysSection,
    pub agent:  AgentSection,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ActiveSection {
    pub provider: Option<String>,
    pub model:    Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct KeysSection {
    pub openai:    Option<String>,
    pub anthropic: Option<String>,
    pub gemini:    Option<String>,
    pub deepseek:  Option<String>,
    pub glm:       Option<String>,
    pub moonshot:  Option<String>,
    // ollama: 키 불필요
}
```

---

## 7. 파일별 구현 순서

1. **`src/config.rs`**: `ForjaConfig` 구조체 재정의, `load_config()`, `save_config()`, `run_onboarding()` 수정, `resolve_api_key()` 헬퍼 추가
2. **`src/main.rs`**: `std::env::args()` 기반 인수 파싱 루틴 추가. `--setup` / `--provider` / `--model` 처리 로직 구현

---

## 8. 환경변수 오버라이드 매핑 (변경 없음)

| 환경변수 | 적용 대상 |
|---|---|
| `FORJA_PROVIDER` | `[active].provider` |
| `FORJA_API_KEY`  | 현재 프로바이더의 키 오버라이드 |
| `FORJA_MODEL`    | `[active].model` |
| `FORJA_USE_MOCK` | Mock 모드 강제 활성화 |
