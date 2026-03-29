[한국어](README.ko.md) | [日本語](README.ja.md) | [中文](README.zh-CN.md) | [Español](README.es.md) | **Português** | [English](../README.md)

This document is a translation of the [English README](../README.md).

# Forja

Um engine de AI agent leve e multiplataforma construído em Rust.

Forja é um AI assistant pessoal que vive no seu terminal. Ele lembra conversas anteriores, detecta contexto emocional, controla seu OS, analisa capturas de tela e adapta a profundidade do seu raciocínio, tudo por linguagem natural.

## Features

**Suporte a LLM com múltiplos providers**
Conecte-se ao OpenAI, Anthropic, Google Gemini, DeepSeek, Moonshot, xAI, GLM ou a modelos locais do Ollama. Troque de provider e model em tempo de execução com `/model`.

**Memory persistente**
Sistema de rolling memory armazenado em markdown. Forja lembra conversas passadas mesmo após reinícios, sem fronteiras de "session".

**Percepção de Emotion e relacionamento**
Detecta sinais emocionais (trabalho tarde da noite, longa ausência, frustração) e ajusta o tom naturalmente.

**Controle do OS**
- Execução de shell command com confirmações de segurança
- Entrada de teclado e mouse (type, click, scroll, hotkeys)
- Automação de browser via CDP (navigate, click, type, read pages, take screenshots)
- Captura de tela + análise com GPT Vision

**Smart Input**
- Arquivos de image por drag-and-drop para análise instantânea com Vision
- `/ss` para captura de tela + análise
- `/image <path>` para análise de image baseada em arquivo
- Multiline input com continuação por `\`

**Thinking adaptativo**
Três modos de raciocínio: `/think min` (conciso), `/think mid` (padrão), `/think max` (raciocínio profundo com self-verification).

**Modos de execução**
`/mode safe` (confirma tudo), `/mode auto` (confirma apenas o perigoso), `/mode trust` (sem confirmações).

**Detecção automática de role**
Alterna automaticamente entre prompts de coder, writer, assistant e analyst com base no contexto da conversa.

**Identity configurável**
Defina o nome do assistant e o user title durante o onboarding. Não há idioma fixo codificado; ele responde no idioma que você usar.

## Quick Start

### Instalação (recomendada)

```bash
cargo install forja
forja                # launch (interactive onboarding on first run)
```

Após a instalação, `forja` fica disponível globalmente a partir de qualquer diretório.

### Instalar a partir do código-fonte

```bash
git clone https://github.com/ai4wm/forja.git
cd forja
cargo run
```

### Pre-built binaries

Você pode baixar versões para Windows, macOS e Linux em [GitHub Releases](https://github.com/ai4wm/forja/releases/latest).

### Setup

```bash
forja setup          # Run setup wizard
forja login openai   # OAuth login
forja login gemini   # OAuth login
forja --provider openai_oauth --model gpt-5.4  # Override at launch
```

## Configuration

Arquivo de configuração: `~/.forja/config.toml`

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
| `FORJA_MODE` | Modo de execução: `safe`, `auto`, `trust` |
| `FORJA_THINK` | Nível de Thinking: `min`, `mid`, `max` |
| `FORJA_ASSISTANT_NAME` | Sobrescreve o nome do assistant |
| `FORJA_USER_TITLE` | Sobrescreve o user title |
| `FORJA_PROVIDER` | Sobrescreve o provider de LLM |
| `FORJA_MODEL` | Sobrescreve o model |
| `FORJA_USE_MOCK` | Executa sem chamadas reais de API |
| `FORJA_VISION` | Ativa/desativa vision (`true`/`false`) |
| `FORJA_BROWSER` | Ativa/desativa o browser tool |
| `FORJA_INPUT` | Ativa/desativa o input tool |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/model <name>` | Troca o model |
| `/models` | Lista os modelos disponíveis |
| `/mode <safe\|auto\|trust>` | Define o modo de execução |
| `/think <min\|mid\|max>` | Define a profundidade do raciocínio |
| `/role <coder\|writer\|assistant\|analyst\|auto>` | Define o role |
| `/ss [prompt]` | Captura de tela + análise com Vision |
| `/image <path> [prompt]` | Analisa um arquivo de image |
| `/help` | Mostra os command disponíveis |

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

Use `/models` em tempo de execução para ver todos os modelos disponíveis.

## Prompt Loading Order

1. Base prompt (identity, regras de memory, regras centrais)
2. Think mode prompt (min/mid/max)
3. Prompt específico de role (coder/writer/assistant/analyst)
4. Descrições das tools
5. Contexto de Emotion
6. Contexto de relacionamento
7. Contexto de conhecimento
8. Contexto de Memory (de memory.md)
9. User global prompt: `~/.forja/USER.md`
10. Project prompt: `AGENTS.md` `FORJA.md` `CLAUDE.md`

## Channels

- **CLI**: Sempre disponível, com streaming output
- **Telegram**: Ative com um bot token. Controle de acesso baseado em whitelist com typing indicators.

## License

MIT OR Apache-2.0

## Links

- [Repository](https://github.com/ai4wm/forja)
- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
