use crate::config::ForjaConfig;
use crate::dashboard::routes::TelegramStatusProvider;
use forja_channel::dashboard_bridge::DashboardBridge;
#[cfg(feature = "discord")]
use forja_channel::discord::DiscordAllowlist;
#[cfg(feature = "discord")]
use forja_channel::multi::DiscordRuntimeConfig;
use forja_channel::multi::MultiChannel;
#[cfg(feature = "voice")]
use forja_channel::voice::VoiceConfig;
use forja_core::traits::{Channel, TelegramConnectionStatus};
#[cfg(feature = "notification")]
use forja_core::traits::{NotificationLevel, NotificationState};
use std::sync::Arc;

pub(crate) struct ChannelBundle {
    pub(crate) channel: Arc<dyn Channel>,
    pub(crate) dashboard_bridge: DashboardBridge,
    pub(crate) interactive_identity_supported: bool,
    pub(crate) print_initial_prompt: bool,
    pub(crate) telegram_status_provider: TelegramStatusProvider,
}

#[cfg_attr(not(feature = "voice"), allow(unused_variables))]
pub(crate) async fn build_channel_bundle(forja_cfg: &ForjaConfig) -> ChannelBundle {
    #[cfg(feature = "telegram")]
    let bot_token = forja_cfg
        .channel
        .telegram
        .bot_token
        .clone()
        .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok());
    #[cfg(not(feature = "telegram"))]
    let bot_token: Option<String> = None;

    #[cfg(feature = "telegram")]
    let allowed_chat_ids = forja_cfg.channel.telegram.allowed_chat_ids.clone();
    #[cfg(not(feature = "telegram"))]
    let allowed_chat_ids = Vec::new();

    #[cfg(feature = "discord")]
    let discord_config = forja_cfg
        .channel
        .discord
        .bot_token
        .clone()
        .or_else(|| std::env::var("DISCORD_BOT_TOKEN").ok())
        .map(|bot_token| DiscordRuntimeConfig {
            bot_token,
            allowlist: DiscordAllowlist {
                allowed_user_ids: forja_cfg.channel.discord.allowed_user_ids.clone(),
                allowed_channel_ids: forja_cfg.channel.discord.allowed_channel_ids.clone(),
                allowed_guild_ids: forja_cfg.channel.discord.allowed_guild_ids.clone(),
            },
        });

    let telegram_requested = bot_token.is_some();
    #[cfg(feature = "telegram")]
    if telegram_requested {
        if allowed_chat_ids.is_empty() {
            println!("[WARN] Telegram allowed_chat_ids is empty.");
        } else {
            println!(
                "MultiChannel starting with CLI + Telegram (IDs: {:?})",
                allowed_chat_ids
            );
        }
    }

    #[cfg(feature = "voice")]
    let voice_config = build_voice_config(forja_cfg);
    #[cfg(feature = "notification")]
    let notification_state = build_notification_state(forja_cfg);
    let multi_channel = Arc::new(
        MultiChannel::new(
            bot_token,
            allowed_chat_ids,
            #[cfg(feature = "discord")]
            discord_config,
            #[cfg(feature = "voice")]
            voice_config,
            #[cfg(feature = "notification")]
            notification_state,
        )
        .await,
    );
    let telegram_status = multi_channel
        .telegram_status()
        .unwrap_or(TelegramConnectionStatus::Disconnected);
    match (telegram_requested, telegram_status) {
        (false, _) => println!("MultiChannel starting with CLI only."),
        (true, TelegramConnectionStatus::Connected) => {
            println!("MultiChannel starting with CLI + Telegram connected.");
        }
        (true, TelegramConnectionStatus::Reconnecting) => {
            println!("MultiChannel starting with CLI + Telegram supervisor (reconnecting).");
        }
        (true, TelegramConnectionStatus::Disconnected) => {
            println!("MultiChannel continuing in CLI-only mode.");
        }
    }

    #[cfg(feature = "telegram")]
    let telegram_status_provider = {
        let telegram_status_handle = multi_channel.telegram_status_handle();
        Arc::new(move || telegram_status_handle.snapshot()) as TelegramStatusProvider
    };
    #[cfg(not(feature = "telegram"))]
    let telegram_status_provider = crate::dashboard::routes::default_telegram_status_provider();

    ChannelBundle {
        channel: multi_channel.clone(),
        dashboard_bridge: multi_channel.dashboard_bridge(),
        interactive_identity_supported: !matches!(
            telegram_status,
            TelegramConnectionStatus::Connected
        ),
        print_initial_prompt: true,
        telegram_status_provider,
    }
}

#[cfg(feature = "notification")]
fn build_notification_state(forja_cfg: &ForjaConfig) -> NotificationState {
    let min_level = match forja_cfg.notification.min_level.to_lowercase().as_str() {
        "info" => NotificationLevel::Info,
        "critical" => NotificationLevel::Critical,
        _ => NotificationLevel::Warning,
    };

    NotificationState {
        enabled: forja_cfg.notification.enabled,
        min_level,
        notify_tasks: forja_cfg.notification.notify_tasks,
        notify_autonomy: forja_cfg.notification.notify_autonomy,
        notify_skills: forja_cfg.notification.notify_skills,
        notify_errors: forja_cfg.notification.notify_errors,
    }
}

#[cfg(feature = "voice")]
fn build_voice_config(forja_cfg: &ForjaConfig) -> Option<VoiceConfig> {
    let api_key = std::env::var("FORJA_VOICE_API_KEY")
        .ok()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .or_else(|| forja_cfg.keys.openai.clone());

    Some(VoiceConfig {
        api_key,
        enabled: matches!(
            std::env::var("FORJA_VOICE_START"),
            Ok(value) if value.eq_ignore_ascii_case("on") || value.eq_ignore_ascii_case("true")
        ),
        transcription_model: std::env::var("FORJA_VOICE_STT_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini-transcribe".to_string()),
        tts_model: std::env::var("FORJA_VOICE_TTS_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini-tts".to_string()),
        tts_voice: std::env::var("FORJA_VOICE_TTS_VOICE").unwrap_or_else(|_| "alloy".to_string()),
        ..VoiceConfig::default()
    })
}
