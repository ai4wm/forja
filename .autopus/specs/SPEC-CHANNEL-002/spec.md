# SPEC-CHANNEL-002: Add Discord Channel Adapter

---
id: SPEC-CHANNEL-002
title: Add Discord Channel Adapter
version: 0.1.0
status: completed
priority: HIGH
---

## Purpose

Extend `forja-channel` with a Discord adapter that follows the existing `Channel` trait model and can participate in Forja's multi-channel runtime.

## Requirements

### Ubiquitous

The system shall provide a Discord adapter behind the existing `discord` feature flag.

The Discord adapter shall translate inbound Discord messages into `forja_core::Message` values and send assistant responses back to the originating Discord target.

The Discord adapter shall enforce explicit access control through configured allowlists rather than accepting arbitrary guild, channel, or user input.

### Event-Driven

WHEN an allowed Discord message arrives THEN the system shall enqueue it for the engine without blocking CLI availability.

WHEN a Discord response is being generated THEN the system shall surface a typing indicator to the active Discord target until the response is sent or canceled.

WHEN the Discord gateway connection drops THEN the adapter shall recover through the Discord client's reconnect lifecycle rather than terminating the overall runtime.

### Unwanted

IF a Discord message comes from a non-allowed source THEN the adapter shall reject it deterministically and avoid routing it into the engine.

IF Discord is unavailable THEN the rest of the runtime shall remain usable from CLI.

## Acceptance Criteria

- [ ] `discord` feature builds successfully.
- [ ] Discord access control is covered by tests.
- [ ] Typing lifecycle is covered by tests or deterministic adapter logic.
- [ ] Runtime integration preserves CLI-first operation when Discord is absent.
