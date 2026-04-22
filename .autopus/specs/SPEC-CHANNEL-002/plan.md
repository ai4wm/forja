# SPEC-CHANNEL-002 Plan: Add Discord Channel Adapter

## File Impact

- `crates/forja-channel/src/discord.rs`
- `crates/forja-channel/src/multi.rs`
- `src/config.rs`
- `src/runtime/boot_channel.rs`
- `crates/forja-channel/src/tests.rs`

## Strategy

1. Add a Discord adapter module behind the existing feature flag.
2. Reuse the `MultiChannel` pattern so Discord remains additive to CLI behavior.
3. Add config-backed allowlists for guilds, channels, and users.
4. Keep Discord-specific behavior isolated from dashboard code.

## Risks

- `serenity` startup/reconnect behavior must not block CLI startup.
- Discord IDs use unsigned snowflakes, so config parsing must stay explicit.
- Typing state must not leak across unrelated sources.
