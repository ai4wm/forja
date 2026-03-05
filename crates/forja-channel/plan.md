# Phase 3: TelegramChannel 구현 계획 (forja-channel)

이 문서는 Phase 3 첫 번째 목표인 Telegram 멀티 채널 연동에 대한 구현 명세서입니다. 기존의 `CliChannel`을 대체/확장하여 텔레그램을 통해 에이전트와 대화할 수 있는 인터페이스를 구축합니다.

## 1. 개요 및 요구사항
- **텔레그램 연동**: `teloxide` 크레이트를 사용하여 비동기 기반의 텔레그램 봇 채널을 구축합니다.
- **채널 추상화**: `forja-core::Channel` 트레이트(`receive`, `send`)를 구현하여 코어 엔진 수정 없이 채널만 교체하여 실행할 수 있도록 합니다.
- **보안 통제**: 퍼블릭 봇 노출을 막기 위해 허용된 `chat_id`에서 온 메시지만 처리하는 화이트리스트 기능을 포함합니다.
- **선택적 빌드**: 바이너리 용량을 줄이고자 `telegram` feature flag 뒤에 배치합니다.

## 2. 파일 및 디렉토리 구조
- **신규 생성 파일**: `crates/forja-channel/src/telegram.rs`
- **수정 대상 파일**: 
  - `crates/forja-channel/Cargo.toml`
  - `crates/forja-channel/src/lib.rs`
  - `src/main.rs`
  - `src/config.rs` (또는 설정 모델부)

## 3. 의존성 구성 (`crates/forja-channel/Cargo.toml`)
```toml
[features]
default = []
telegram = ["dep:teloxide"]

[dependencies]
# 2026년 기준 호환 최신 버전 (예: 0.13.x 이상) 확인 적용
teloxide = { version = "0.13", features = ["macros", "rustls"], optional = true }
tokio = { version = "1.0", features = ["sync"] }
```

## 4. 아키텍처 및 메시지 큐 구조 (`telegram.rs`)
`forja-core::Channel` 트레이트는 `async fn receive(&self) -> Result<Message>`를 무한 루프 내에서 폴링하는 구조입니다. 또한, `send(msg)` 시 대상 `chat_id`를 알기 위해 `Message` 코어 구조체를 건드리는 대신, `TelegramChannel` 내부에 마지막 송신자의 `last_chat_id` 상태를 캐싱하는 방식으로 안전하게 구현합니다.

### 4.1. 구조체 명세
```rust
pub struct TelegramChannel {
    bot: teloxide::Bot,
    receiver: tokio::sync::Mutex<mpsc::Receiver<(i64, Message)>>,
    last_chat_id: tokio::sync::Mutex<Option<i64>>,
    allowed_chat_ids: Vec<i64>,
}
```

### 4.2. 동작 흐름
- **메시지 수신 (Bot -> mpsc::Sender)**: 
  - 텔레그램 봇 루프(`teloxide::repl`)가 백그라운드 태스크로 구동됩니다.
  - 봇 핸들러가 수신한 `message.chat.id`를 화이트리스트와 대조합니다.
  - 통과 시 `Message::text()` 포맷으로 변환한 뒤, `(chat_id, message)` 튜플을 `mpsc::Sender`를 통해 큐에 밀어 넣습니다.
- **`Channel::receive()` 구현**:
  - `mpsc::Receiver`에서 `recv().await`로 하나씩 메시지를 꺼냅니다.
  - 반환 직전 `last_chat_id` 값을 방금 꺼낸 `chat_id`로 갱신하고 `Message`만 엔진으로 넘깁니다.
- **`Channel::send()` 구현**:
  - 엔진이 `Message`를 보내면, 내부 `last_chat_id` 락을 열어 대상 ID를 가져옵니다.
  - `bot.send_message(chat_id, text).await`를 호출해 텔레그램 방으로 실제 답변을 전송합니다.

## 5. 인증 및 설정 연동
### 5.1 토큰 및 화이트리스트 공급
`TELEGRAM_BOT_TOKEN` 환경 변수 또는 `config.toml`의 `[channel.telegram]` 섹션을 통해 봇 토큰과 허용된 사용자 ID 배열을 로드합니다.

```toml
# config.toml 예시
[channel.telegram]
bot_token = "123456789:ABCDefgh..."
allowed_chat_ids = [ 12345678 ]
```

### 5.2 보안 화이트리스트 (Required)
- `TelegramChannel`은 인가되지 않은 ID가 봇에게 메시지를 보낼 경우 `"[DENIED] Authorized users only."`라는 경고를 텔레그램 상으로 남기고, 내부 큐(`mpsc::Sender`)로는 메시지를 전달하지 않습니다.

## 6. CLI 통합 (`main.rs`)
- 커맨드 라인 인수에 `--channel` 플래그를 추가합니다. 
- 예: `cargo run -- --channel telegram`
- 실행 분기 로직:
  ```rust
  let channel: Arc<dyn Channel> = if args.channel == Some("telegram".into()) {
      let tg_config = load_telegram_config(&forja_cfg);
      Arc::new(TelegramChannel::new(tg_config).await)
  } else {
      Arc::new(CliChannel)
  };
  
  let mut engine = Engine::new(provider, channel);
  ```

이 문서를 바탕으로 Phase 3의 텔레그램 연동 구현을 단계적으로 진행합니다.
