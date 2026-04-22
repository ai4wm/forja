use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use forja_core::error::{ForjaError, Result};
use forja_core::traits::{Channel, VoiceChannelStatus};
use forja_core::types::{Content, Message, Role};
use reqwest::multipart::{Form, Part};
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};

const DEFAULT_CAPTURE_SECS: u64 = 4;
const DEFAULT_SAMPLE_RATE: u32 = 16_000;

#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub api_base: String,
    pub api_key: Option<String>,
    pub transcription_model: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub capture_secs: u64,
    pub sample_rate: u32,
    pub enabled: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: None,
            transcription_model: "gpt-4o-mini-transcribe".to_string(),
            tts_model: "gpt-4o-mini-tts".to_string(),
            tts_voice: "alloy".to_string(),
            capture_secs: DEFAULT_CAPTURE_SECS,
            sample_rate: DEFAULT_SAMPLE_RATE,
            enabled: false,
        }
    }
}

struct VoiceRuntime {
    config: VoiceConfig,
    enabled: AtomicBool,
    shutdown: AtomicBool,
    status: StdMutex<VoiceChannelStatus>,
    playback_sink: StdMutex<Option<Arc<Sink>>>,
}

pub struct VoiceChannel {
    runtime: Arc<VoiceRuntime>,
    receiver: Mutex<mpsc::Receiver<Message>>,
}

impl VoiceChannel {
    pub fn new(config: VoiceConfig) -> Self {
        let runtime = Arc::new(VoiceRuntime {
            enabled: AtomicBool::new(config.enabled),
            shutdown: AtomicBool::new(false),
            status: StdMutex::new(if config.enabled {
                VoiceChannelStatus::Listening
            } else {
                VoiceChannelStatus::Disabled
            }),
            playback_sink: StdMutex::new(None),
            config,
        });
        let (tx, rx) = mpsc::channel(16);
        spawn_voice_loop(runtime.clone(), tx);
        Self {
            runtime,
            receiver: Mutex::new(rx),
        }
    }

    fn set_status(&self, status: VoiceChannelStatus) {
        if let Ok(mut current) = self.runtime.status.lock() {
            *current = status;
        }
    }

    fn stop_playback(&self) {
        if let Ok(mut sink) = self.runtime.playback_sink.lock()
            && let Some(sink) = sink.take()
        {
            sink.stop();
        }
        if self.runtime.enabled.load(Ordering::SeqCst) {
            self.set_status(VoiceChannelStatus::Listening);
        }
    }

    async fn speak_text(&self, text: &str) -> Result<()> {
        let Some(api_key) = self.runtime.config.api_key.clone() else {
            return Err(ForjaError::ChannelError(
                "Voice output is unavailable because no OpenAI API key is configured".to_string(),
            ));
        };

        self.stop_playback();
        self.set_status(VoiceChannelStatus::Speaking);
        let bytes = request_tts_bytes(&self.runtime.config, &api_key, text).await?;
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            if let Err(error) = play_audio_bytes(runtime.clone(), bytes) {
                eprintln!("[Voice] playback failed: {error}");
                if let Ok(mut status) = runtime.status.lock() {
                    *status = VoiceChannelStatus::Unavailable;
                }
            }
        });
        Ok(())
    }
}

#[async_trait]
impl Channel for VoiceChannel {
    async fn receive(&self) -> Result<Message> {
        let mut receiver = self.receiver.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| ForjaError::ChannelError("Voice channel receiver closed".to_string()))
    }

    async fn send(&self, message: Message) -> Result<()> {
        if !self.runtime.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Content::Text { text, .. } = &message.content
            && matches!(message.role, Role::Assistant | Role::System)
        {
            self.speak_text(text).await?;
        }
        Ok(())
    }

    fn shutdown(&self) {
        self.runtime.shutdown.store(true, Ordering::SeqCst);
        self.runtime.enabled.store(false, Ordering::SeqCst);
        self.stop_playback();
        self.set_status(VoiceChannelStatus::Disabled);
    }

    async fn cancel_typing(&self) {
        self.stop_playback();
    }

    fn supports_voice(&self) -> bool {
        true
    }

    fn voice_status(&self) -> Option<VoiceChannelStatus> {
        self.runtime.status.lock().ok().map(|status| *status)
    }

    async fn set_voice_enabled(&self, enabled: bool) -> Result<VoiceChannelStatus> {
        if enabled && self.runtime.config.api_key.is_none() {
            self.set_status(VoiceChannelStatus::Unavailable);
            return Ok(VoiceChannelStatus::Unavailable);
        }

        self.runtime.enabled.store(enabled, Ordering::SeqCst);
        let status = if enabled {
            VoiceChannelStatus::Listening
        } else {
            self.stop_playback();
            VoiceChannelStatus::Disabled
        };
        self.set_status(status);
        Ok(status)
    }
}

fn spawn_voice_loop(runtime: Arc<VoiceRuntime>, tx: mpsc::Sender<Message>) {
    tokio::spawn(async move {
        loop {
            if runtime.shutdown.load(Ordering::SeqCst) {
                break;
            }
            if !runtime.enabled.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }

            if let Ok(mut status) = runtime.status.lock() {
                *status = VoiceChannelStatus::Listening;
            }

            let capture_config = runtime.config.clone();
            let audio = tokio::task::spawn_blocking(move || capture_audio_wav(&capture_config))
                .await
                .ok()
                .and_then(Result::ok);

            let Some(audio) = audio else {
                if let Ok(mut status) = runtime.status.lock() {
                    *status = VoiceChannelStatus::Unavailable;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let Some(api_key) = runtime.config.api_key.clone() else {
                if let Ok(mut status) = runtime.status.lock() {
                    *status = VoiceChannelStatus::Unavailable;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let text = transcribe_audio(&runtime.config, &api_key, audio).await;
            match text {
                Ok(text) if !text.trim().is_empty() => {
                    if let Ok(mut sink) = runtime.playback_sink.lock()
                        && let Some(sink) = sink.take()
                    {
                        sink.stop();
                    }
                    let _ = tx.send(Message::text(Role::User, text, None)).await;
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("[Voice] transcription failed: {error}");
                    if let Ok(mut status) = runtime.status.lock() {
                        *status = VoiceChannelStatus::Unavailable;
                    }
                }
            }
        }
    });
}

fn capture_audio_wav(config: &VoiceConfig) -> Result<Vec<u8>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| ForjaError::ChannelError("No microphone input device found".to_string()))?;
    let supported_config = device.default_input_config().map_err(|error| {
        ForjaError::ChannelError(format!("Could not query input config: {error}"))
    })?;
    let sample_format = supported_config.sample_format();
    let stream_config = supported_config.config();
    let channels = stream_config.channels as usize;
    let samples = Arc::new(StdMutex::new(Vec::<i16>::new()));
    let samples_for_stream = samples.clone();
    let error_fn = move |error| {
        eprintln!("[Voice] input stream error: {error}");
    };

    let stream = match sample_format {
        cpal::SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| collect_i16_samples(data, channels, &samples_for_stream),
            error_fn,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _| collect_u16_samples(data, channels, &samples_for_stream),
            error_fn,
            None,
        ),
        cpal::SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| collect_f32_samples(data, channels, &samples_for_stream),
            error_fn,
            None,
        ),
        _ => {
            return Err(ForjaError::ChannelError(
                "Unsupported microphone sample format".to_string(),
            ));
        }
    }
    .map_err(|error| ForjaError::ChannelError(format!("Failed to build input stream: {error}")))?;

    stream.play().map_err(|error| {
        ForjaError::ChannelError(format!("Failed to start input stream: {error}"))
    })?;
    std::thread::sleep(Duration::from_secs(config.capture_secs));
    drop(stream);

    let captured = samples
        .lock()
        .map_err(|error| ForjaError::ChannelError(error.to_string()))?
        .clone();
    if captured.is_empty() || average_amplitude(&captured) < 150 {
        return Err(ForjaError::ChannelError(
            "No usable voice input detected".to_string(),
        ));
    }

    let mut cursor = Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: config.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|error| {
            ForjaError::ChannelError(format!("Failed to create WAV writer: {error}"))
        })?;
        for sample in captured {
            writer.write_sample(sample).map_err(|error| {
                ForjaError::ChannelError(format!("Failed to encode WAV sample: {error}"))
            })?;
        }
        writer.finalize().map_err(|error| {
            ForjaError::ChannelError(format!("Failed to finalize WAV data: {error}"))
        })?;
    }

    Ok(cursor.into_inner())
}

async fn transcribe_audio(config: &VoiceConfig, api_key: &str, audio: Vec<u8>) -> Result<String> {
    let part = Part::bytes(audio)
        .file_name("voice.wav")
        .mime_str("audio/wav")
        .map_err(|error| ForjaError::ChannelError(format!("Invalid WAV mime type: {error}")))?;
    let form = Form::new()
        .text("model", config.transcription_model.clone())
        .part("file", part);
    let endpoint = format!(
        "{}/audio/transcriptions",
        config.api_base.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|error| {
            ForjaError::ChannelError(format!("Voice transcription request failed: {error}"))
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ForjaError::ChannelError(format!(
            "Voice transcription failed with HTTP {}: {}",
            status, body
        )));
    }
    let payload: serde_json::Value = response.json().await.map_err(|error| {
        ForjaError::ChannelError(format!("Invalid transcription response: {error}"))
    })?;
    Ok(payload["text"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string())
}

async fn request_tts_bytes(config: &VoiceConfig, api_key: &str, text: &str) -> Result<Vec<u8>> {
    let endpoint = format!("{}/audio/speech", config.api_base.trim_end_matches('/'));
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": config.tts_model,
            "voice": config.tts_voice,
            "input": text,
            "response_format": "wav"
        }))
        .send()
        .await
        .map_err(|error| {
            ForjaError::ChannelError(format!("Voice synthesis request failed: {error}"))
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ForjaError::ChannelError(format!(
            "Voice synthesis failed with HTTP {}: {}",
            status, body
        )));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| ForjaError::ChannelError(format!("Could not read TTS audio: {error}")))
}

fn play_audio_bytes(runtime: Arc<VoiceRuntime>, bytes: Vec<u8>) -> Result<()> {
    let cursor = Cursor::new(bytes);
    let (_stream, handle) = OutputStream::try_default().map_err(|error| {
        ForjaError::ChannelError(format!("No speaker output device found: {error}"))
    })?;
    let sink = Arc::new(Sink::try_new(&handle).map_err(|error| {
        ForjaError::ChannelError(format!("Could not create audio sink: {error}"))
    })?);
    let source = Decoder::new(cursor).map_err(|error| {
        ForjaError::ChannelError(format!("Could not decode TTS audio: {error}"))
    })?;
    sink.append(source);
    {
        let mut playback = runtime
            .playback_sink
            .lock()
            .map_err(|error| ForjaError::ChannelError(error.to_string()))?;
        *playback = Some(sink.clone());
    }
    sink.sleep_until_end();
    if let Ok(mut playback) = runtime.playback_sink.lock() {
        *playback = None;
    }
    if let Ok(mut status) = runtime.status.lock() {
        *status = if runtime.enabled.load(Ordering::SeqCst) {
            VoiceChannelStatus::Listening
        } else {
            VoiceChannelStatus::Disabled
        };
    }
    Ok(())
}

fn average_amplitude(samples: &[i16]) -> i16 {
    if samples.is_empty() {
        return 0;
    }
    let total = samples
        .iter()
        .map(|sample| i32::from(sample.unsigned_abs()))
        .sum::<i32>();
    (total / samples.len() as i32) as i16
}

fn collect_i16_samples(input: &[i16], channels: usize, output: &Arc<StdMutex<Vec<i16>>>) {
    if let Ok(mut buffer) = output.lock() {
        for frame in input.chunks(channels.max(1)) {
            if let Some(sample) = frame.first() {
                buffer.push(*sample);
            }
        }
    }
}

fn collect_u16_samples(input: &[u16], channels: usize, output: &Arc<StdMutex<Vec<i16>>>) {
    if let Ok(mut buffer) = output.lock() {
        for frame in input.chunks(channels.max(1)) {
            if let Some(sample) = frame.first() {
                buffer.push((*sample as i32 - i32::from(u16::MAX) / 2) as i16);
            }
        }
    }
}

fn collect_f32_samples(input: &[f32], channels: usize, output: &Arc<StdMutex<Vec<i16>>>) {
    if let Ok(mut buffer) = output.lock() {
        for frame in input.chunks(channels.max(1)) {
            if let Some(sample) = frame.first() {
                buffer.push((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
            }
        }
    }
}
