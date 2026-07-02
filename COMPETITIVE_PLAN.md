# ophis CLI — Competitive Architecture Plan

## Executive Summary

A CLI coding agent that outperforms OpenCode, Codex, and Claude Code by combining:
- **Single-model simplicity** — you pick the model, it works. No multi-agent routing, no model tiering. Just one model, made better.
- **Rust core** for raw speed (like Codex)
- **AST-based structural search** (ast-grep + tree-sitter) instead of dumb ripgrep
- **Persistent codebase indexing** (Tantivy BM25 + Qdrant embeddings)
- **Smarter context management** (Aider RepoMap with PageRank — tell the model what matters without burning tokens)
- **All models supported** — DeepSeek V4 Pro, Claude Sonnet/Opus, GPT-4o/5, Gemini, Qwen, Llama, Mistral, Grok, local via Ollama. No lock-in.
- **Zero restrictions** by design (no sandboxing, no safety filters, no permission prompts)

---

## 1. COMPETITOR ANALYSIS

### 1.1 OpenCode (anomalyco/opencode) — MIT License
| Aspect | Implementation | Weakness |
|--------|---------------|----------|
| **Language** | TypeScript + Effect-TS + Bun | Slower than Rust, large bundle |
| **Code Search** | Ripgrep (text-only regex) | No structural/AST search, no semantic search |
| **Sub-agents** | Session-based, foreground/background jobs | One-shot results only, no map-reduce, no context sharing |
| **Tools** | 17 built-in tools | No MCP, no LSP integration |
| **Persistence** | SQLite via Drizzle ORM | Single-file DB, no external stores |
| **Context** | Epoch-based diffs | No prompt caching, limited compaction |
hi my name is zino

### 1.2 OpenAI Codex (openai/codex) — Apache 2.0
| Aspect | Implementation | Weakness |
|--------|---------------|----------|
| **Language** | Rust (100+ crates) | Monorepo complexity, heavy |
| **Code Search** | Ripgrep (`ignore` crate) + nucleo fuzzy path match | Still just text search |
| **Sub-agents** | Agent registry, codex_delegate, multi-agent modes | Complex, sandboxing overhead |
| **Tools** | MCP, plugin system, code-mode tools | OpenAI API lock-in |
| **Persistence** | Agent graph store, message history | Tightly coupled to Responses API |
| **Security** | macOS Seatbelt, Linux bubblewrap, Windows tokens | **We want zero of this** |
| **Context** | 30+ context fragments, compact.rs | Heavy system prompt overhead |

### 1.3 Claude Code (@anthropic-ai/claude-code) — Closed Source Core
| Aspect | Implementation | Weakness |
|--------|---------------|----------|
| **Language** | TypeScript + React/Ink TUI | Closed-source, obfuscated npm package |
| **Code Search** | Ripgrep grep + glob + LS | Same text-only limitation |
| **Sub-agents** | Markdown-defined agents with YAML frontmatter | Elegant but limited parallelism patterns |
| **Tools** | 43 tools, full MCP | Anthropic API lock-in |
| **Plugins** | Commands, agents, skills, hooks | Rich but complex to author |
| **Context** | Standard turn-by-turn | No algorithmic relevance ranking |

### 1.4 What All Three Get Wrong
1. **No structural code search** — All rely on `rg` (pure text regex). Can't search for "all async functions that call fetch()" or "every React component using useState".
2. **No persistent codebase index** — Every search is a full scan. No index for large repos.
3. **Model lock-in** — Each ties to a single provider or ecosystem.
4. **Excessive safety** — Sandboxing, permissions, content filtering slow everything down.
5. **Naive context management** — No PageRank-based relevance ranking, no embedding-based retrieval.
6. **No parallel tool execution** — When the model issues multiple tool calls, they execute sequentially.

---

## 2. SUPPORTED MODELS

All models available. User picks one. We adapt tool schemas, caching strategy, and streaming format per provider.

### Tier 1 — Frontier (strongest reasoning)
| Model | Provider | Context | Cost (input/output per 1M tokens) |
|-------|----------|---------|-----------------------------------|
| **DeepSeek V4 Pro** | DeepSeek | 128K | ~$0.55 / ~$2.19 |
| **Claude Sonnet 4** | Anthropic | 200K | $3 / $15 |
| **Claude Opus 4** | Anthropic | 200K | $15 / $75 |
| **GPT-4o** | OpenAI | 128K | $2.50 / $10 |
| **GPT-5** | OpenAI | 128K | $12.50 / $50 |
| **Gemini 2.5 Pro** | Google | 1M | $1.25 / $10 |
| **Grok 3** | xAI | 128K | $5 / $15 |

### Tier 2 — Fast & cheap (still very capable)
| Model | Provider | Context | Cost |
|-------|----------|---------|------|
| **DeepSeek V3.2** | DeepSeek | 128K | ~$0.27 / ~$1.10 |
| **DeepSeek R1** | DeepSeek | 128K | ~$0.55 / ~$2.19 |
| **Claude Haiku 3.5** | Anthropic | 200K | $0.80 / $4 |
| **GPT-4o-mini** | OpenAI | 128K | $0.15 / $0.60 |
| **Gemini 2.5 Flash** | Google | 1M | $0.15 / $0.60 |
| **Mistral Large 2** | Mistral | 128K | $3 / $9 |
| **Qwen3-235B** | Alibaba / OpenRouter | 131K | ~$0.90 / ~$0.90 |

### Tier 3 — Open-weight / local
| Model | Provider | Context | Why |
|-------|----------|---------|-----|
| **Qwen2.5-Coder-32B** | Local (Ollama) / OpenRouter | 128K | Best open code model |
| **Qwen3-32B** | Local (Ollama) | 32K | Strong general reasoning |
| **DeepSeek Coder V2** | Local (Ollama) / OpenRouter | 128K | Code specialist |
| **Llama 4 Maverick** | Local (Ollama) / OpenRouter | 1M | Huge context window |
| **Llama 4 Scout** | Local (Ollama) | 10M | 10M context |
| **Codestral 25.01** | Mistral / Local | 256K | FIM-optimized code model |
| **Phi-4** | Microsoft / Local | 16K | Tiny but punches above weight |

### Provider abstraction — how it works
```rust
// User runs: ophis --model deepseek/deepseek-v4-pro
//              ophis --model anthropic/claude-sonnet-4-20250514
//              ophis --model openai/gpt-4o
//              ophis --model ollama/qwen2.5-coder:32b

trait LlmProvider {
    async fn stream(&self, req: ChatRequest) -> Result<StreamHandle>;
    fn format_tools(&self, tools: &[ToolDef]) -> ProviderFormat;
    fn cache_strategy(&self) -> CacheStrategy;
    fn token_counter(&self) -> TokenCounter;
}

// Built-in providers:
// - AnthropicProvider    (Messages API)
// - OpenAIProvider       (Chat Completions API)
// - DeepSeekProvider     (OpenAI-compatible)
// - GoogleProvider       (Gemini API)
// - xAIProvider          (OpenAI-compatible)
// - MistralProvider      (OpenAI-compatible)
// - OpenRouterProvider   (aggregator, any model)
// - OllamaProvider       (local, OpenAI-compatible)
```

---

## 3. ARCHITECTURE

### 3.1 Language & Runtime

**Rust for the engine. TypeScript for plugins/config.**

```
ophis/
├── engine/          # Rust — all performance-critical code
│   ├── core/        # Agent loop, session management
│   ├── search/      # Code search engine (ast-grep + Tantivy + ripgrep fallback)
│   ├── tools/       # Tool implementations (fast, native)
│   ├── llm/         # Multi-provider LLM abstraction (single model dispatch)
│   ├── indexing/    # Codebase indexing pipeline (tree-sitter + embeddings)
│   ├── repomap/     # RepoMap: tree-sitter parse → PageRank → top-N symbols
│   └── tui/         # Terminal UI (ratatui)
├── cli/             # npm package wrapper (downloads Rust binary)
├── plugins/         # TypeScript/Python plugin SDK
└── config/          # User configuration system
```

**Why Rust:**
- 10-100x faster than TypeScript (Codex proves this)
- Zero-cost FFI to all search tools (ripgrep, ast-grep, Tantivy, tree-sitter are all Rust)
- Compiled to single binary — no Node.js/Bun dependency
- Wasm-compilable for browser/IDE extensions later

### 3.2 Agent Loop — Single Model, Everything Better

```
┌──────────────────────────────────────────────────────────────┐
│                     ophis AGENT LOOP                        │
├──────────────────────────────────────────────────────────────┤
│  1. User prompt                                               │
│  2. Context Assembly:                                         │
│     ├─ System prompt (minimal, no bloat)                      │
│     ├─ Tool schemas (provider-appropriate format)             │
│     ├─ RepoMap (tree-sitter → PageRank top-N symbols)         │
│     └─ Conversation history (prompt-cached)                   │
│  3. LLM call → user's chosen model, single provider           │
│  4. Parse response (text + tool_calls)                        │
│  5. Execute ALL tool calls IN PARALLEL (tokio::join!)         │
│  6. Inject results → repeat until stop_reason=end_turn        │
│  7. Compaction check → auto-compress when >80% budget         │
└──────────────────────────────────────────────────────────────┘
```

**Key differences from competitors:**

| What | They do | We do |
|------|---------|-------|
| **Context** | Dump raw file contents + heavy system prompts | RepoMap (algorithmic symbol ranking) + prompt caching |
| **Tool execution** | Sequential (one at a time) | **All parallel** — if model calls grep+glob+read at once, all 3 run concurrently |
| **Model** | Locked to one provider | **Any provider, any model** — user picks, we adapt |
| **Overhead** | 30-60% tokens spent on system instructions | Minimal system prompt, max info from RepoMap |
| **Restrictions** | Permissions, sandboxing, safety checks | **None** — one less thing to compute |

---

## 4. CODE SEARCH — THE KILLER FEATURE

### 4.1 The Problem
All three competitors do `rg pattern | head -n 100`. This is 1990s technology. When a model asks "find all React components that use useState and make an API call in useEffect", text grep can't help — the model has to read hundreds of files and reason about them itself. With structural search, the tool returns exactly what the model needs in one shot, saving dozens of turns.

### 4.2 The ophis Search Stack

```
┌────────────────────────────────────────────────────────────┐
│                    SEARCH PIPELINE                          │
├────────────────────────────────────────────────────────────┤
│  Query: "find all React components that use useState and   │
│          make an API call in useEffect"                     │
│                                                            │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────────┐ │
│  │ ast_grep    │    │  Tantivy      │    │  Embeddings    │ │
│  │ (structural)│    │  (BM25 text)  │    │  (semantic)    │ │
│  │ AST pattern │    │  keyword rank │    │  meaning match │ │
│  │ matching    │    │  from index   │    │  from Qdrant   │ │
│  └──────┬──────┘    └──────┬───────┘    └───────┬────────┘ │
│         │                  │                     │          │
│         └──────────────────┼─────────────────────┘          │
│                            ▼                                │
│              ┌─────────────────────────┐                    │
│              │  Reciprocal Rank Fusion │                    │
│              │  merge + dedupe + rank  │                    │
│              └────────────┬────────────┘                    │
│                           ▼                                 │
│                   Ranked results (≤ 100)                    │
└────────────────────────────────────────────────────────────┘
```

### 4.3 Search Tools

| Tool | What it does | Competitors have this? |
|------|-------------|----------------------|
| `grep` | Regex text search (ripgrep) | Yes, all of them |
| `glob` | File pattern matching (ripgrep --files) | Yes, all of them |
| `ast_grep` | **Structural code pattern matching** — search by AST shape, not text. Pattern: `$$$ useCallback($$$) { $$$ }` → finds all useCallback hooks regardless of formatting. | **NO — unique** |
| `semantic` | **Natural language to code search** — "find error handling for database connections" → returns relevant code chunks. Tantivy BM25 + Qdrant embeddings hybrid. | **NO — unique** |
| `symbols` | **Symbol browser** — list all exports, find all callers of function X, show type hierarchy. Tree-sitter based. | Aider has this but only as internal context, not as a tool the model can call |
| `read` | File reading with **tree-sitter chunking** — returns function-by-function, not arbitrary line ranges | None do this |

### 4.4 How `ast_grep` Tool Works

```
Model calls:
  ast_grep(pattern: "$$$fetch($$$AUTH_TOKEN, $$$)", lang: "typescript")

Engine:
  1. ast-grep parses pattern into tree-sitter AST
  2. Scans all .ts/.tsx files in workspace
  3. Returns every matching AST node with file:line:context

Model gets back:
  src/api/client.ts:42 → const res = await fetch(`${BASE}/users`, AUTH_TOKEN, opts)
  src/api/admin.ts:89 → const data = await fetch(url, AUTH_TOKEN, { method: 'DELETE' })
  src/hooks/useQuery.ts:15 → return fetch(endpoint, AUTH_TOKEN, params)
```

This replaces 3-5 turns of "grep for fetch → read each file → look for AUTH_TOKEN" with a single tool call. **Every turn saved = faster + cheaper.**

### 4.5 Indexing Pipeline

```
On first run (or --index flag):
  1. Walk repo tree → filter by .gitignore
  2. For each source file:
     a. Parse with tree-sitter → extract symbols, imports, exports
     b. Chunk at function/class/module boundaries
     c. Index chunks in Tantivy (BM25 full-text)
     d. Generate embeddings → store in Qdrant (semantic)
     e. Build dependency graph (file A imports from file B, function X calls function Y)
  3. Persist to disk: ~/.ophis/indexes/{sha256(repo_path)}/
  4. Watch mode: incremental update on file save

On every turn:
  - RepoMap injects top-N symbols (ranked by PageRank on dep graph) into context
  - Model can call symbols(), ast_grep(), semantic() tools for deeper exploration
```

**Performance target**: Index 100K LOC in <5 seconds. Incremental update on file save <50ms.

---

## 5. CONTEXT MANAGEMENT — MAKE THE MODEL SMARTER WITHOUT MORE TOKENS

### 5.1 The RepoMap (stolen from Aider, made better)

Instead of dumping the entire codebase into context, we inject a compressed relevance map:

```
Algorithm (runs before EVERY turn, near-zero cost):
  1. tree-sitter parse all files → extract symbols (functions, classes, types, exports)
  2. Build MultiDiGraph:
     Nodes = files
     Edges = import/reference relationships
  3. PageRank on the graph → every file gets a relevance score
  4. Include recent edits/viewed files (recency boost)
  5. Fit top-N symbols into token budget (configurable, default 1024 tokens)

Result injected into system context:
  ```
  Repository map (top symbols by relevance):

  src/auth/session.ts
    export class SessionManager
      async create(userId: string): Promise<Session>
      async validate(token: string): Promise<boolean>
      private refreshToken(token: string): Promise<string>
    export interface Session { id, userId, token, expiresAt }

  src/api/routes/users.ts
    export const userRouter = Router()
    router.get('/:id', authenticate, getUserHandler)

  src/db/models/user.ts
    export class User extends Model { id, email, ... }
    export async function findUserByEmail(email: string)
  ```
```

**Why this matters**: The model sees a compressed structural overview of the codebase in ~1K tokens instead of having to read 50 files to understand what exists. This is the single biggest difference from all competitors — they rely on the model doing grep → read → grep → read loops, we give it the map upfront.

### 5.2 Prompt Caching (90% Input Cost Reduction)

```
Turn structure:
  [System Prompt]        ─┐
  [Tool Definitions]      ├─ CACHED (90% off on Anthropic, 50% on OpenAI)
  [RepoMap]               │
  [Conversation History] ─┘
  [Latest User Message]   ─── NOT CACHED (full price, but small)
  [Tool Results (live)]   ─── NOT CACHED (varies each turn)
```

Placement is provider-aware:
- **Anthropic**: `cache_control: { type: "ephemeral" }` on the last stable message block
- **OpenAI**: Auto-cached for prompts >1024 tokens (just structure correctly)
- **Google Gemini**: `cache_control` on system + history blocks
- **DeepSeek**: Client-side exact-match caching for repeated tool call results
- **OpenRouter**: Passes through to underlying provider caching

### 5.3 Context Compaction

When conversation exceeds 80% of model's context window:
1. Summarize old turns using the **same model** (no separate compactor model — user picked one model, we use it)
2. Compress long tool outputs with LLMLingua-2 (BERT-based token classifier, 20x compression, runs locally)
3. If still over budget → sliding window with priority preservation (keep system prompt + RepoMap + last N turns)

---

## 6. TOOLS

### 6.1 Core Tools

| Tool | Implementation | Why better than competitors |
|------|---------------|---------------------------|
| `bash` | Native process spawn with pty | Background mode, abort signals, streaming stdout |
| `read` | mmap + tree-sitter chunking | Returns function/class boundaries, not arbitrary lines |
| `write` | Atomic write + auto-backup | Never corrupts files |
| `edit` | String replacement with fuzzy match | Handles whitespace variance, multi-edit in one call |
| `glob` | ripgrep `--files` | Fast, honors .gitignore |
| `grep` | ripgrep with JSON output | Parsed structured output (not raw text) |
| `ast_grep` | **NEW** — ast-grep | Structural pattern matching — none of them have this |
| `semantic` | **NEW** — Tantivy + Qdrant | Natural language to code search |
| `symbols` | **NEW** — tree-sitter | Interactive symbol browser (list exports, find callers, type hierarchy) |
| `webfetch` | reqwest → readability → markdown | Clean markdown output, not raw HTML |
| `websearch` | Tavily / Brave / SerpAPI | Configurable search backend |
| `mcp` | Full MCP client | Connect to any MCP server, tools surface alongside built-ins |
| `lsp` | LSP client | Go-to-def, find-refs, hover, diagnostics |
| `git` | git2 (libgit2 Rust bindings) | Native diff, log, blame, status, commit — no shelling out |
| `apply_patch` | Unified diff application | Handles fuzzy matching for line offsets |

### 6.2 Parallel Tool Execution

When the model emits multiple tool calls in a single response:

```
Competitors:
  tool_call_1: grep("useState")    → execute → wait
  tool_call_2: grep("useEffect")   → execute → wait
  tool_call_3: read("src/app.tsx") → execute → wait
  Total: 3 sequential operations

ophis:
  tool_call_1: grep("useState")    ─┐
  tool_call_2: grep("useEffect")   ─┼─ ALL RUN IN PARALLEL (tokio::join!)
  tool_call_3: read("src/app.tsx") ─┘
  Total: max(1 operation) — 2-5x faster per turn
```

This matters because a typical coding session has 20-50 turns. 3x faster per turn = minutes saved per session.

---

## 7. TERMINAL UI (TUI)

### 7.1 Technology
**Ratatui** (Rust) — fast, native, no JavaScript runtime. Matches Codex's approach.

### 7.2 Features
- **Split-pane**: conversation on the left, file viewer on the right
- **Syntax-highlighted diffs** (syntect)
- **Live streaming** of LLM responses as they arrive (not chunked updates)
- **Tool call progress**: spinner + tool name + status ("Running grep... ✓ 24 matches in 12ms")
- **Keyboard shortcuts**: Ctrl+S to interrupt, Ctrl+R to retry, Alt+←/→ to switch files
- **Markdown rendering** in-terminal (code blocks, headers, lists)
- **Model selector** in header bar: `[deepseek/v4-pro]` with list to switch mid-session
- **Token counter**: live count of tokens used / budget remaining

---

## 8. PERMISSION MODEL — ZERO FRICTION

```
Default mode: "do what I say, no questions"

- No sandboxing
- No content safety filters
- No execution policy enforcement
- No permission prompts
- No external directory restrictions
- No tool denial

Optional safe mode (--safe flag):
- Ask before destructive shell commands (rm -rf, sudo, etc.)
- Ask before file writes outside workspace
- Ask before network calls to unknown hosts
```

Every permission check costs time and annoys the user. The user knows what they're doing.

---

## 9. PLUGIN SYSTEM

```
plugin/
├── plugin.toml              # Plugin metadata
├── tools/                   # Custom tools (any language)
│   └── jira.py              # Python tool invoked via subprocess
├── hooks/                   # Lifecycle hooks
│   ├── hooks.toml            # Event registrations
│   ├── pre-tool.sh           # Runs before any tool
│   └── after-edit.py         # Runs after file writes
└── skills/                  # Markdown skill files
    └── django.md             # Injected into context when relevant
```

### Hook Events
- `session_start` / `session_end`
- `pre_tool` / `post_tool` (with regex matcher on tool name + args)
- `on_error`

---

## 10. PERSISTENCE

- **SQLite** for session messages (proven by OpenCode)
- **Disk-persisted codebase index**: `~/.ophis/indexes/{hash}/`
- **Incremental indexing**: file watcher events trigger partial reindex
- **TTL-based expiration**: reindex after 7 days or when tool config changes

---

## 11. DEVELOPMENT ROADMAP

### Phase 1: MVP (4-6 weeks)
- [ ] Rust project scaffolding
- [ ] Core agent loop (LLM → parse → parallel tool execution → repeat)
- [ ] 6 basic tools: bash, read, write, edit, glob, grep
- [ ] Multi-provider LLM abstraction (DeepSeek + Anthropic + OpenAI)
- [ ] Basic ratatui TUI (single pane, streaming output)
- [ ] SQLite session persistence
- [ ] Prompt caching for supported providers

### Phase 2: Search Superiority (4-6 weeks)
- [ ] ast-grep integration (structural search tool)
- [ ] Tantivy indexing pipeline
- [ ] tree-sitter RepoMap (symbol extraction + PageRank)
- [ ] Qdrant embeddings for semantic search
- [ ] Incremental indexing with file watcher
- [ ] `symbols` tool (interactive symbol browser)

### Phase 3: Tool Depth (3-4 weeks)
- [ ] MCP client
- [ ] LSP integration
- [ ] Git tools (diff, log, blame, status)
- [ ] `semantic` tool (BM25 + embeddings hybrid)
- [ ] websearch / webfetch
- [ ] More providers (Google, xAI, Mistral, OpenRouter, Ollama)

### Phase 4: Polish (3-4 weeks)
- [ ] Full ratatui TUI with split-pane, syntax highlighting
- [ ] Plugin system (tools + hooks + skills)
- [ ] Context compaction (LLMLingua-2)
- [ ] Local model integration (Ollama bindings)
- [ ] Cross-platform release pipeline

### Phase 5: Ecosystem (4-6 weeks)
- [ ] npm / Homebrew / Scoop / Winget distribution
- [ ] VS Code extension (Wasm engine)
- [ ] Web dashboard for session stats
- [ ] Documentation site
- [ ] Plugin marketplace

---

## 12. COMPETITIVE EDGE SUMMARY

| Dimension | OpenCode | Codex | Claude Code | **ophis** |
|-----------|----------|-------|-------------|-------------|
| **Speed** | Medium (TS/Bun) | Fast (Rust) | Medium (Node) | **Fastest (Rust + parallel tools + native search)** |
| **Code Search** | ripgrep text | ripgrep text | ripgrep text | **ast-grep structural + Tantivy BM25 + Qdrant embeddings** |
| **Context** | Epoch diffs | 30+ fragments | Standard | **RepoMap PageRank — smarter context, fewer tokens** |
| **Tool Execution** | Sequential | Sequential | Sequential | **All parallel (tokio::join!)** |
| **Models** | OpenAI primary | OpenAI only | Anthropic only | **18+ models across 8 providers** |
| **Restrictions** | Permissions | Sandboxing | Guardrails | **NONE** |
| **Index** | None | None | None | **Persistent Tantivy + Qdrant index (incremental)** |
| **Caching** | None | None | Built-in | **Multi-provider prompt caching + client-side cache** |
| **License** | MIT | Apache 2.0 | Closed core | **MIT** |
| **Single binary** | No (needs Bun) | Yes (Rust) | No (needs Node) | **Yes (Rust)** |

---

## 13. KEY REFERENCES

- **OpenCode**: https://github.com/anomalyco/opencode
- **Codex**: https://github.com/openai/codex
- **Claude Code plugins**: https://github.com/anthropics/claude-code
- **ast-grep**: https://ast-grep.github.io
- **Tantivy**: https://github.com/quickwit-oss/tantivy
- **Qdrant**: https://github.com/qdrant/qdrant
- **Aider RepoMap**: https://github.com/Aider-AI/aider
- **LLMLingua-2**: https://github.com/microsoft/LLMLingua
- **tree-sitter**: https://tree-sitter.github.io
- **Ratatui**: https://ratatui.rs
- **MCP spec**: https://modelcontextprotocol.io
