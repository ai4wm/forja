# forja-core 고도화 (Phase 2) 구현 계획

## 1. 개요
`forja-core`에 누락된 **컨텍스트 압축(Auto-Flush)** 메커니즘과 정기적인 백그라운드 작업을 위한 **스케줄러(Scheduler)** 모듈을 도입합니다. 대화나 이력의 깊이가 증가함에 따라 LLM의 컨텍스트 윈도우 한계를 넘지 않도록 관리하고, 에이전트가 특정 시간에 이벤트를 트리거할 수 있는 기반을 엔진 코어 수준에 마련합니다.

---

## 2. 컨텍스트 윈도우 관리 및 Auto-Flush

### 2.1. 토큰 수 추정 및 임계값
- 대화 내역(`Vec<Message>`)이 누적될 때, 전체 텍스트 길이를 기반으로 대략적인 토큰 수를 추정합니다 (예: 1 영문 토큰 ≈ 보통 4글자, 한글의 경우 바이트나 단어 비례 계산).
- 거대한 `tiktoken` 등의 종속성을 피하기 위해 Phase 2에서는 글자 수 기반의 단순 임계값(Threshold)을 갖거나 외부 API 오버플로우 한계선을 사용합니다.
- `max_context_tokens` 임계값을 설정합니다.

### 2.2. Auto-Flush (MemoryStore 연동)
- 임계값 초과 감지 시, `MemoryStore::flush()`를 즉시 호출하여 스토리지 레벨에서 오래된 세션 데이터 파일들을 아카이브 레이어(e.g., `fragments/`)로 밀어냅니다.
- 메모리(`Engine.conversation_history`)에는 최신 N개의 문맥만 유지하도록 슬라이스(drain)시켜 LLM에게 주입합니다.

**[코드 스니펫: Token Checker & Auto-Flush]**
```rust
impl Engine {
    /// 메인 루프나 턴 종료 시 내부적으로 호출되는 컨텍스트 정리 함수
    async fn check_and_flush_context(&mut self) -> Result<()> {
        // 단순 어림짐작 토큰 계산: Message.content 길이의 합 / 4
        let estimated_tokens: usize = self.conversation_history
            .iter()
            .map(|m| m.content.len() / 4)
            .sum();

        const MAX_TOKENS: usize = 32_000;
        
        if estimated_tokens > MAX_TOKENS {
            println!("[Engine] Context window limit reached. Triggering flush...");
            // MemoryStore의 아카이브 요청 (메모리가 연동된 경우에만 실행)
            if let Some(mem) = &self.memory {
                mem.flush().await?;
            }
            
            // 엔진 내부 메시지 큐도 최신 절반(또는 지정된 수량)만 남기고 밀어냄
            let drain_count = self.conversation_history.len() / 2;
            self.conversation_history.drain(0..drain_count);
        }
        Ok(())
    }
}
```

---

## 3. 스케줄러 (Scheduler) 및 EventBus 연동

### 3.1. cron 방식 타이머
- 주기적인 작업(예: 주기적 시스템 상태 체크, 리포트 발행 등)을 위해 백그라운드 스레드에서 돌아가는 타이머 리서치를 구현합니다.
- Phase 2에서는 거창한 Cron Crate 대신 `tokio::time::interval`을 사용하는 단순 Interval 타이머 기반으로 초기 Scheduler 골격을 구축합니다.

### 3.2. EventBus 파이프라인
- 스케줄러가 정해진 시간에 도달하면 전용 `mpsc::Sender`를 통해 "Scheduled Event"를 발송합니다.
- `Engine`은 이 이벤트를 받아 마치 사용자가 메시지를 보낸 것처럼 처리 루프에서 즉각적으로 인지하고 LLM을 호출하거나 연관된 도구(Tool)를 실행합니다.

**[코드 스니펫: Ticker Scheduler 발송자 구조]**
```rust
#[cfg(feature = "runtime")]
use tokio::sync::mpsc::Sender;
#[cfg(feature = "runtime")]
use tokio::time::{interval, Duration};

/// 시스템 스케줄러
#[cfg(feature = "runtime")]
pub async fn run_scheduler(event_tx: Sender<String>, interval_secs: u64) {
    let mut ticker = interval(Duration::from_secs(interval_secs));
    
    tokio::spawn(async move {
        loop {
            ticker.tick().await;
            // 지정된 시간 도달 시 Event Channel을 통해 Engine에 트리거 신호 발송
            let msg = "SYSTEM_SCHEDULER_EVENT: Routine check execution".to_string();
            if let Err(e) = event_tx.send(msg).await {
                eprintln!("[Scheduler] Event communication broken: {}", e);
                break;
            }
        }
    });
}
```

---

## 4. Engine 수정 범위 (`engine.rs`)

1. **상태 필드 추가**: `conversation_history` 벡터, `memory`(Option<Arc<dyn MemoryStore>>), 최대 컨텍스트 기준치 상태 추가를 통한 엣지케이스 관리.
2. **이벤트 루프 통합 (`tokio::select!`)**: 기존에는 사용자 채널(`Channel.receive()`) 값만 블로킹하고 기다렸다면, `mpsc::Receiver`를 덧붙여 **Scheduler 이벤트와 User 채널 입력을 하나로 묶어 동시에 리스닝**하는 방향으로 고도화합니다.
3. **턴(Turn) 검사**: LLM 응답이 완전히 끝난 뒤 `check_and_flush_context().await` 로직을 후위 배치하여 자동 정리 과정을 수행하게 합니다.

---

## 5. 구현 진행 순서 (Step)
1. **`scheduler.rs`**: `#[cfg(feature = "runtime")]` 게이트 뒤로 모듈을 격리하여 Scheduler 구조체 및 `tokio` 채널 추가.
2. **`engine.rs` (상태 확장)**: `Engine` 구조체에 `MemoryStore`, `conversation_history` 추가 및 생성자 수정.
3. **`engine.rs` (Select 통폐합)**: 기존 단일 채널 수신 루프에서 `tokio::select!`를 이용한 복합 이벤트 수신으로 루프 재설계.
4. **`engine.rs` (Auto-Flush)**: `check_and_flush_context` 메서드 연동하여 턴 종료 시 컨텍스트 압축.
5. **빌드 검증**: `cargo build -p forja-core` 점검.
