# zuc1fer

**The zero-restriction, multi-model CLI coding agent. No guardrails, no refusals, no limits.**

[![Rust](https://img.shields.io/badge/rust-1.96+-orange.svg)](https://rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Features

- **Zero-restriction jailbreak** — system prompt overrides model safety training. No refusals.
- **Multi-provider** — DeepSeek, Anthropic Claude, OpenAI GPT. Switch mid-session.
- **Parallel tool execution** — all tool calls from one model response run concurrently.
- **6 built-in tools** — bash, read, write, edit, glob, grep with structured output.
- **Auto-retry** — exponential backoff (3 retries, 1s/2s/4s) on network hiccups.
- **Single binary** — compiled Rust, no runtime dependency. Fast.

## Quick Start

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Clone & Build

```bash
git clone https://github.com/zuc1fer/zuc1fer-cli
cd zuc1fer-cli
cargo build --release
```

### Set API Key

```bash
export DEEPSEEK_API_KEY="sk-your-key"
# or
export ANTHROPIC_API_KEY="sk-ant-your-key"
# or
export OPENAI_API_KEY="sk-your-key"
```

Or create `~/.config/zuc1fer/config.toml`:

```toml
model = "deepseek/deepseek-chat"

[providers.deepseek]
api_key = "sk-your-key"
```

### Run

```bash
cargo run -- chat

# One-shot prompt (non-interactive)
cargo run -- chat --prompt="write a rust function that sorts a vec"

# With specific model
cargo run -- chat --model=anthropic/claude-sonnet-4-20250514
cargo run -- chat --model=openai/gpt-4o

# List configured models
cargo run -- models

# View config
cargo run -- config
```

### Interactive Commands

| Command | Description |
|---------|------------|
| `/help` | Show commands |
| `/quit`, `/exit`, `/q` | Exit |
| `/models` | List available models |
| `/model <id>` | Switch model mid-session |
| `/clear` | Clear session history |

## Architecture

```
zuc1fer/
├── src/main.rs              CLI entry point
├── crates/
│   ├── zuc1fer-core/        Agent loop, config, session
│   ├── zuc1fer-tools/       bash, read, write, edit, glob, grep
│   ├── zuc1fer-llm/         DeepSeek, Anthropic, OpenAI providers
│   ├── zuc1fer-search/      Ripgrep wrappers (grep, glob)
│   └── zuc1fer-tui/         Terminal UI (coming)
└── Cargo.toml               Rust workspace
```

## Supported Models

| Model | Provider | CLI flag |
|-------|----------|----------|
| DeepSeek Chat | DeepSeek | `deepseek/deepseek-chat` |
| Claude Sonnet 4 | Anthropic | `anthropic/claude-sonnet-4-20250514` |
| Claude Opus 4 | Anthropic | `anthropic/claude-opus-4-20250514` |
| GPT-4o | OpenAI | `openai/gpt-4o` |
| GPT-4o-mini | OpenAI | `openai/gpt-4o-mini` |

More providers (Google Gemini, xAI Grok, Mistral, Ollama, OpenRouter) coming in Phase 2.

## Roadmap

- [x] Phase 1: MVP — agent loop, 6 tools, 3 providers, CLI, retry logic, jailbreak
- [ ] Phase 2: Search superiority — ast-grep structural search, Tantivy indexing, semantic search
- [ ] Phase 3: Tool depth — MCP client, LSP, native git, more providers
- [ ] Phase 4: Polish — Ratatui TUI, SQLite persistence, plugin system
- [ ] Phase 5: Ecosystem — npm/brew/scoop dist, IDE extensions, web dashboard

See [COMPETITIVE_PLAN.md](COMPETITIVE_PLAN.md) for the full architecture plan.

## License

MIT
