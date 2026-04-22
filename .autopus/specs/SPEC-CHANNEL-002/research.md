# SPEC-CHANNEL-002 Research: Add Discord Channel Adapter

## Findings

- Root `Cargo.toml` already exposes a `discord` feature mapped to `forja-channel/discord`.
- `forja-channel` already contains the Telegram supervisor and `MultiChannel` orchestration pattern.
- `forja-core` already exposes `ChannelKind::Discord`, but the gateway adapter lacks a Discord adapter type.
- Runtime config currently models only Telegram channel settings, so Discord requires parallel config support.

## Recommended Shape

- Keep the Discord implementation feature-gated inside `forja-channel`.
- Reuse `MultiChannel` as the runtime entry point instead of replacing it.
- Treat allowlists as explicit configuration rather than heuristic access checks.
