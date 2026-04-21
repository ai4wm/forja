# Product Overview

## Purpose

Forja is a local-first AI assistant that runs from the terminal and extends into a small set of optional companion surfaces.
The current product shape is a personal agent runtime that combines:

- multi-provider LLM access
- persistent local memory
- execution tools for the OS and browser
- autonomous task tracking
- a local dashboard over runtime state

## Primary User

The repository and runtime behavior indicate a developer-oriented primary user:

- someone working from a terminal
- someone who wants local persistence under `~/.forja`
- someone who may switch providers and models during a session
- someone who benefits from OS-level tooling, browser automation, and memory recall

## Core Features

- Interactive terminal assistant started with `forja`
- Provider onboarding through `forja setup`
- OAuth or token-based login through `forja login <provider>`
- Runtime model discovery and switching through `/models` and `/model`
- Execution controls through `/mode` and `/think`
- Background structured-memory maintenance with idle and manual `/dream` execution
- Visual analysis through `/ss` and `/image`
- Debate and task capture through `/debate` and `/task`
- Local dashboard through `/dashboard`
- Tool-backed actions through shell, browser, input, vision, file, web, and search integrations
- Telegram side-channel support when configured

## Product Modes

### Execution Modes

- `safe`: confirm all risky actions
- `auto`: confirm only dangerous actions
- `trust`: run without confirmation gates

### Reasoning Modes

- `min`
- `mid`
- `max`

### Role Modes

- `auto`
- `coder`
- `writer`
- `assistant`
- `analyst`

### Channel Modes

- CLI-only
- CLI plus Telegram when a bot token and allowlist are configured

## Main User Flows

### 1. First-Time Setup

The user runs `forja setup`, configures providers, selects a default model, and sets assistant and user labels.

### 2. Login and Session Start

The user logs into OpenAI, Gemini, or Anthropic if needed, then starts `forja` and begins an interactive session.

### 3. Assisted Work

The user asks for help in natural language, changes models or modes on demand, and invokes screenshots, image analysis, debate mode, or direct tasks.

### 4. Runtime Review

The user opens `/dashboard` to inspect audit logs, debates, budgets, tasks, skills, and unresolved items stored in `audit.db`.

## Boundaries

- The product is local-first, not a hosted SaaS application.
- The dashboard is a companion view, not a separate web product.
- The current repository does not define container or platform deployment artifacts.
- The code exposes Telegram support, but CLI remains the primary and best-covered interaction path.
