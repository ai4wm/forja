[한국어](README.ko.md) | [日本語](README.ja.md) | [中文](README.zh-CN.md) | **Español** | [Português](README.pt-BR.md) | [English](../README.md)

This document is a translation of the [English README](../README.md).

# Forja

Un motor de AI agent ligero y multiplataforma construido en Rust.

Forja es un AI assistant personal que vive en tu terminal. Recuerda conversaciones pasadas, detecta el contexto emocional, controla tu OS, analiza capturas de pantalla y adapta la profundidad de su razonamiento, todo mediante lenguaje natural.

## Features

**Soporte multi-provider de LLM**
Conéctate a OpenAI, Anthropic, Google Gemini, DeepSeek, Moonshot, xAI, GLM o modelos locales de Ollama. Cambia de provider y model en tiempo de ejecución con `/model`.

**Memory persistente**
Sistema de rolling memory almacenado en markdown. Forja recuerda conversaciones anteriores incluso después de reiniciarse, sin límites de "session".

**Conciencia de Emotion y relación**
Detecta señales emocionales (trabajo nocturno, ausencia prolongada, frustración) y ajusta el tono de forma natural.

**Control del OS**
- Ejecución de shell command con confirmaciones de seguridad
- Entrada de teclado y mouse (type, click, scroll, hotkeys)
- Automatización de browser con CDP (navigate, click, type, read pages, take screenshots)
- Captura de pantalla + análisis con GPT Vision

**Smart Input**
- Archivos de image por drag-and-drop para análisis instantáneo con Vision
- `/ss` para captura de pantalla + análisis
- `/image <path>` para análisis de image basado en archivo
- Multiline input con continuación usando `\`

**Thinking adaptativo**
Tres modos de razonamiento: `/think min` (conciso), `/think mid` (predeterminado), `/think max` (razonamiento profundo con self-verification).

**Modos de ejecución**
`/mode safe` (confirma todo), `/mode auto` (confirma solo lo peligroso), `/mode trust` (sin confirmaciones).

**Detección automática de role**
Cambia automáticamente entre los prompts de coder, writer, assistant y analyst según el contexto de la conversación.

**Identity configurable**
Configura el nombre del assistant y el user title durante el onboarding. No hay un idioma codificado de forma fija; responde en el idioma que uses.

## Quick Start

### Instalar desde el código fuente

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

La primera ejecución inicia el onboarding wizard para configurar tu provider, el nombre del assistant y tus preferencias.

### Instalar desde crates.io

```bash
cargo install forja
```

### Setup

```bash
forja setup          # Run setup wizard
forja login openai   # OAuth login
forja login gemini   # OAuth login
forja --provider openai_oauth --model gpt-5.4  # Override at launch
```

## Configuration

Archivo de configuración: `~/.forja/config.toml`

```toml
[active]
provider = "openai_oauth"
model = "gpt-5.4"

[identity]
assistant_name = "Forja"
user_title = "User"

[keys]
openai = "sk-..."
anthropic = "sk-ant-..."

[channel.telegram]
bot_token = "123456:token"
allowed_chat_ids = [123456789]
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `FORJA_MODE` | Modo de ejecución: `safe`, `auto`, `trust` |
| `FORJA_THINK` | Nivel de Thinking: `min`, `mid`, `max` |
| `FORJA_ASSISTANT_NAME` | Sobrescribe el nombre del assistant |
| `FORJA_USER_TITLE` | Sobrescribe el user title |
| `FORJA_PROVIDER` | Sobrescribe el provider de LLM |
| `FORJA_MODEL` | Sobrescribe el model |
| `FORJA_USE_MOCK` | Ejecuta sin llamadas reales a la API |
| `FORJA_VISION` | Activa/desactiva vision (`true`/`false`) |
| `FORJA_BROWSER` | Activa/desactiva el browser tool |
| `FORJA_INPUT` | Activa/desactiva el input tool |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | Cambia el model |
| `/models` | Lista los modelos disponibles |
| `/mode <safe\|auto\|trust>` | Configura el modo de ejecución |
| `/think <min\|mid\|max>` | Configura la profundidad del razonamiento |
| `/role <coder\|writer\|assistant\|analyst\|auto>` | Configura el role |
| `/ss [prompt]` | Captura de pantalla + análisis con Vision |
| `/image <path> [prompt]` | Analiza un archivo de image |
| `/help` | Muestra los command disponibles |

## Architecture

```text
forja/
 src/main.rs              # Entry point, onboarding, tool registration
 crates/
    forja-core/          # Engine loop, prompt assembly, mode system
    forja-llm/           # Multi-provider LLM client
    forja-memory/        # Markdown + BM25 memory store
    forja-tools/         # Shell, input, browser, vision, search tools
    forja-channel/       # CLI and Telegram channels
```

## Supported Providers

OpenAI (API & OAuth), Anthropic, Google Gemini (API & OAuth), DeepSeek, Moonshot, xAI, GLM, Ollama.

Usa `/models` en tiempo de ejecución para ver todos los modelos disponibles.

## Prompt Loading Order

1. Base prompt (identity, reglas de memory, reglas principales)
2. Think mode prompt (min/mid/max)
3. Prompt específico por role (coder/writer/assistant/analyst)
4. Descripciones de tools
5. Contexto de Emotion
6. Contexto de relación
7. Contexto de conocimiento
8. Contexto de Memory (desde memory.md)
9. User global prompt: `~/.forja/USER.md`
10. Project prompt: `AGENTS.md` `FORJA.md` `CLAUDE.md`

## Channels

- **CLI**: Siempre disponible, con streaming output
- **Telegram**: Actívalo con un bot token. Control de acceso basado en whitelist con typing indicators.

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
