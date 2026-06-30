# zuc1fer

**A fast, multi-model CLI coding agent. Single Rust binary, structural + semantic search, MCP and LSP built in.**

[![Rust](https://img.shields.io/badge/rust-1.96+-orange.svg)](https://rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Features

- **Direct, no-friction operation** — a minimal, capable system prompt; the agent acts instead of asking.
- **Multi-provider** — DeepSeek, Anthropic Claude, OpenAI, OpenRouter, and local Ollama. Switch mid-session.
- **Rich tool set** — bash, read, write, edit, glob, grep, ast-grep, semantic search, git, LSP, web fetch/search, plus MCP servers and plugins.
- **Parallel reads, serialized writes** — independent read-only tool calls run concurrently; mutating calls (`write`/`edit`/`bash`) run in order.
- **Auto-retry** — exponential backoff (3 retries) on network hiccups.
- **Single Rust binary** — fast and compiled. (`grep`/`glob`/`ast_grep` shell out to `rg`/`sg` when used.)

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
├── src/main.rs              CLI + TUI entry point
├── crates/
│   ├── zuc1fer-core/        Agent loop, config, sessions, code index, RepoMap, LSP
│   ├── zuc1fer-tools/       bash, read, write, edit, glob, grep, ast_grep, git, web, semantic
│   ├── zuc1fer-llm/         DeepSeek, Anthropic, OpenAI, OpenRouter, Ollama providers
│   ├── zuc1fer-mcp/         MCP client (stdio JSON-RPC)
│   └── zuc1fer-tui/         Ratatui terminal UI
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

OpenRouter and local Ollama are also supported. Google Gemini, xAI Grok, and Mistral are not yet implemented.

## Roadmap

- [x] Phase 1: MVP — agent loop, tools, multi-provider, CLI, retry logic
- [x] Phase 2: Search — ast-grep structural search, Tantivy indexing, semantic search, RepoMap
- [x] Phase 3: Tool depth — MCP client, LSP, native git, OpenRouter + Ollama
- [x] Phase 4: Polish — Ratatui TUI, SQLite persistence, plugin system
- [ ] Phase 5: Ecosystem — npm/brew/scoop dist, IDE extensions, web dashboard

See [COMPETITIVE_PLAN.md](COMPETITIVE_PLAN.md) for the full architecture plan.

## License

MIT
