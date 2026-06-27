# Architecture

## Crate Dependency Graph

```
src/main.rs (CLI binary)
  └── zuc1fer-core (agent, config, session)
        ├── zuc1fer-tools (tool implementations + registry)
        ├── zuc1fer-llm (provider abstraction)
        │     └── providers/ (deepseek, anthropic, openai)
        └── zuc1fer-search (ripgrep wrappers)

zuc1fer-tui (terminal UI, placeholder)
```

## Agent Loop

```
User prompt
  → Session.add_message (user)
  → loop:
      → session.to_llm_messages()
      → provider.stream_chat() [spawned in tokio task]
      → event_rx.recv() [realtime streaming]
      → if text → print & buffer
      → if tool_calls → collect
      → if Done → accumulate usage
      → if Error/transient → retry with backoff (3 attempts)
      → if no tool_calls → return response
      → ToolRegistry.execute_parallel(tool_calls) [tokio::join!]
      → session.add_message (assistant + tool_calls)
      → session.add_message (tool results)
      → repeat
```

## Tool Execution

All tool calls from a single model response execute in parallel via `futures::future::join_all`. Each tool gets a fresh async task. Results are collected and injected back into the conversation as individual tool messages.

## Provider Abstraction

`LlmProvider` trait:
- `stream_chat(request, event_tx)` — sends SSE events through channel
- `provider_name()` — e.g. "deepseek"
- `default_model()` — e.g. "deepseek-chat"
- `supports_prompt_caching()` — provider capability flag
- `estimate_tokens(text)` — rough token count

Each provider handles:
- Tool schema conversion (native format)
- Message format conversion (role + content → API-specific JSON)
- Streaming event normalization (API-specific SSE → StreamEvent enum)

## Session Format

Sessions are plain Rust structs (serde-serializable). Ready for SQLite persistence (Phase 4). Messages track role, text content, tool calls, and tool results separately for clean API format conversion.

## Config

`~/.config/zuc1fer/config.toml` — TOML format, auto-created on first run. Supports per-provider API keys and base URL overrides (useful for proxies and OpenRouter).
