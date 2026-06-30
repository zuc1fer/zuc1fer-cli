# Hierarchical / Tree-Structured Memory for LLM Agents: A Research Report

**Date:** 2025-06-27  
**Scope:** MemGPT/Letta and mem0 — concrete numbers only, with source URLs.

---

## 1. MemGPT / Letta — Virtual Context Management

### 1.1 Paper & Sources

- **Paper:** "MemGPT: Towards LLMs as Operating Systems" — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560)
- **Research site:** [https://research.memgpt.ai](https://research.memgpt.ai)
- **Letta (formerly MemGPT) GitHub:** [https://github.com/letta-ai/letta](https://github.com/letta-ai/letta)
- **Letta docs — context hierarchy:** [https://docs.letta.com/guides/core-concepts/memory/context-hierarchy](https://docs.letta.com/guides/core-concepts/memory/context-hierarchy)
- **Letta docs — memory blocks:** [https://docs.letta.com/guides/core-concepts/memory/memory-blocks](https://docs.letta.com/guides/core-concepts/memory/memory-blocks)
- **Letta docs — compaction:** [https://docs.letta.com/guides/core-concepts/messages/compaction](https://docs.letta.com/guides/core-concepts/messages/compaction)

### 1.2 OS-Inspired Memory Hierarchy

MemGPT models the LLM's context window as a constrained memory resource, analogous to physical RAM in an operating system. It introduces a two-tier memory hierarchy:

| Tier | Analogy | What it holds |
|------|---------|---------------|
| **Main Context** (in-context working memory) | Physical RAM | System instructions + working context + FIFO message queue. The LLM "sees" this on every turn. |
| **External Context** (archival + recall storage) | Disk | Archival storage (read/write DB of arbitrary-length text objects) + recall storage (FIFO queue eviction archive). Accessed via function calls. |

From the paper (§2.1, §2.2):

> "We treat context windows as a constrained memory resource, and design a memory hierarchy for LLMs analogous to memory tiers used in traditional OSes."  
> — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), p.2

The Letta API (v1) further formalized this into:

- **Memory Blocks** — always in-context (no retrieval needed). Recommended limit: `<50k characters` per block, `<20 blocks` per agent. Stored as XML-like `<memory_blocks>` prepended to the prompt.
- **Archival Memory** — unlimited storage, accessed via `archival_memory_insert` / `archival_memory_search` tools. Passage limit: `300 tokens` each.
- **Files** — partial access; opened/closed by tools. Size limit: `5MB` per file, `<100 files` per agent.
- **External RAG** — unlimited via MCP or custom tools.

Source: [Letta Context Hierarchy docs](https://docs.letta.com/guides/core-concepts/memory/context-hierarchy)

### 1.3 Virtual Context / Paging Mechanism

MemGPT "pages" data in/out of the context window via LLM function calls:

1. **Memory pressure warning**: When prompt tokens exceed the *warning token count* (~70% of the context window), a system message warns the LLM to store important info.
2. **Queue eviction**: When prompt tokens exceed the *flush token count* (100%), the queue manager evicts ~50% of messages, generates a new recursive summary, and stores old messages in recall storage (indefinitely accessible via function calls).
3. **Self-directed retrieval**: The LLM can call `archival_storage.search()` or `conversation_search()` to pull data back into main context — paginated to avoid overflow.

From the paper (§2.2):

> "When the prompt tokens exceed the 'flush token count' (e.g. 100% of the context window), the queue manager flushes the queue to free up space in the context window: the queue manager evicts a specific count of messages (e.g. 50% of the context window), generates a new recursive summary using the existing recursive summary and evicted messages."  
> — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), p.3

In the current Letta Code product, the `system/` directory concept refines this: files in `system/` are always loaded into the system prompt; files outside are visible as a tree structure but loaded only on-demand.

Source: [Letta Code Memory docs](https://docs.letta.com/letta-code/memory)

### 1.4 The "Infinite Context" Illusion — Token Reduction

MemGPT provides the illusion of unbounded context by keeping only a fixed-size window in the LLM prompt. The paper does **not** report explicit token-cost savings percentages. However, it demonstrates that:

- The fixed-context baseline must fit all data into the context window. When documents exceed the window, truncation degrades accuracy.
- MemGPT never exceeds its base model's context window (e.g., 8k tokens for GPT-4), yet can query archives of **20 million Wikipedia articles**.

> "[MemGPT] is able to scale to larger effective context lengths. MemGPT actively retrieves documents from its archival storage (and can iteratively page through results), so the total number of documents available to MemGPT is no longer limited by the number of documents that fit within the LLM processor's context window."  
> — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), p.6

**Explicit token numbers are NOT documented in the MemGPT paper.**

### 1.5 Document QA Benchmarks

**Multi-Document QA task** (NaturalQuestions-Open, Wikipedia dump, 50 questions):

The paper reports accuracy as a function of K (number of retrieved documents). Results from Figure 5:

| Method | Accuracy behavior as K grows |
|--------|------------------------------|
| **GPT-4 (fixed-context)** | ~25% at K=1, rises to ~45% at K=30+, plateaus |
| **GPT-4 Turbo (fixed-context)** | ~30% at K=1, rises to ~50% at K=30+, plateaus |
| **MemGPT (GPT-4)** | **~70%** consistently, regardless of K |
| **MemGPT (GPT-4 Turbo)** | **~70%** consistently, regardless of K |
| **MemGPT (GPT-3.5)** | Significantly degraded (~35-40%) due to limited function-calling |
| **Fixed-context + truncation** | Accuracy drops to ~5-20% as documents are compressed |

Source: [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), Figure 5, p.6

Key quote:

> "MemGPT's performance is unaffected by increased context length. Methods such as truncation can extend the effective context lengths of fixed length models such as GPT-4, but such compression methods will lead to performance degradation as the necessary compression grows."  
> — [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), p.6

### 1.6 Multi-Session Chat Benchmarks

**Deep Memory Retrieval (DMR)** task — agent is asked a specific question about a topic from sessions 1–5:

| Model | Accuracy | ROUGE-L (R) |
|-------|----------|-------------|
| GPT-3.5 Turbo | 38.7% | 0.394 |
| GPT-3.5 Turbo + MemGPT | **66.9%** | **0.629** |
| GPT-4 | 32.1% | 0.296 |
| GPT-4 + MemGPT | **92.5%** | **0.814** |
| GPT-4 Turbo | 35.3% | 0.359 |
| GPT-4 Turbo + MemGPT | **93.4%** | **0.827** |

Source: [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), Table 2, p.5

**Conversation Opener** task (engagement, SIM-1 cosine similarity to gold persona):

| Method | SIM-1 | SIM-3 | SIM-H |
|--------|-------|-------|-------|
| Human (gold) | 0.800 | 0.800 | 1.000 |
| GPT-3.5 Turbo | 0.830 | 0.812 | 0.817 |
| GPT-4 | 0.868 | 0.843 | 0.773 |
| GPT-4 Turbo | 0.857 | 0.828 | 0.767 |

MemGPT agents **exceed human-written openers** in SIM-1 for all underlying models.

Source: [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), Table 3, p.5

**Nested Key-Value Retrieval** task — multi-hop lookup across up to 4 nesting levels:

| Method | Accuracy at nesting level 0 | At level 2 | At level 4 |
|--------|------------------------------|------------|------------|
| GPT-3.5 (baseline) | ~70% | **0%** | **0%** |
| GPT-4 (baseline) | ~90% | ~40% | **0%** |
| GPT-4 Turbo (baseline) | ~90% | ~60% | ~20% |
| MemGPT (GPT-4) | ~100% | **~100%** | **~100%** |
| MemGPT (GPT-4 Turbo) | ~100% | ~80% | ~60% |
| MemGPT (GPT-3.5) | ~80% | ~50% | ~20% |

Source: [arXiv:2310.08560](https://arxiv.org/abs/2310.08560), Figure 7, p.7

---

## 2. mem0 — Scalable Memory-Centric Architecture

### 2.1 Paper & Sources

- **Paper:** "Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory" — [arXiv:2504.19413](https://arxiv.org/abs/2504.19413)
- **HTML version:** [https://arxiv.org/html/2504.19413v1](https://arxiv.org/html/2504.19413v1)
- **Research page:** [https://mem0.ai/research](https://mem0.ai/research)
- **GitHub:** [https://github.com/mem0ai/mem0](https://github.com/mem0ai/mem0)
- **Eval framework:** [https://github.com/mem0ai/memory-benchmarks](https://github.com/mem0ai/memory-benchmarks)

### 2.2 Headline Claims vs Full-Context Baseline

From the paper abstract (verbatim):

> "Mem0 attains a **91% lower p95 latency** and saves **more than 90% token cost**, thereby offering a compelling balance between advanced reasoning capabilities and practical deployment constraints."  
> — [arXiv:2504.19413](https://arxiv.org/abs/2504.19413)

> "Notably, Mem0 achieves **26% relative improvements in the LLM-as-a-Judge metric over OpenAI**, while Mem0 with graph memory achieves around **2% higher overall score** than the base Mem0 configuration."  
> — [arXiv:2504.19413](https://arxiv.org/abs/2504.19413)

### 2.3 Performance Comparison — All Methods (LOCOMO Benchmark, Overall J Score)

From Table 2 in Section 4.3 — Overall LLM-as-a-Judge (J) scores:

| Method | Overall J Score |
|--------|----------------|
| Full-context | **72.90 ± 0.19%** |
| Mem0^g (graph) | **68.44 ± 0.17%** |
| Mem0 | **66.88 ± 0.15%** |
| Zep | 65.99 ± 0.16% |
| LangMem | 58.10 ± 0.21% |
| OpenAI (ChatGPT memory) | 52.90 ± 0.14% |
| A-Mem (re-run) | 48.38 ± 0.15% |

Best RAG k=2: 60.97 ± 0.20% (chunk size 256)

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 2, Section 4.3

Key quote:

> "Even the strongest RAG approach peaks at around **61%** in the J metric. Mem0 reaches **67%** — about a **10% relative improvement**. Mem0^g reaches over **68%**, achieving around a **12% relative gain**."  
> — [arXiv:2504.19413](https://arxiv.org/abs/2504.19413) Section 4.3

**Note:** Full-context achieves the highest J score (72.90%), but at dramatically higher cost (see latency/token sections below).

### 2.4 Per-Question-Type J Scores

From Table 1 — LLM-as-a-Judge (J) scores by question category:

| Method | Single-Hop | Multi-Hop | Open-Domain | Temporal |
|--------|-----------|-----------|-------------|----------|
| Mem0 | **67.13 ± 0.65** | **51.15 ± 0.31** | 72.93 ± 0.11 | **55.51 ± 0.34** |
| Mem0^g | 65.71 ± 0.45 | 47.19 ± 0.67 | **75.71 ± 0.21** | **58.13 ± 0.44** |
| OpenAI (full) | 63.79 ± 0.46 | 42.92 ± 0.63 | 62.29 ± 0.12 | 21.71 ± 0.20 |
| Zep | 61.70 ± 0.32 | 41.35 ± 0.48 | **76.60 ± 0.13** | 49.31 ± 0.50 |
| LangMem | 62.23 ± 0.75 | 47.92 ± 0.47 | 71.12 ± 0.20 | 23.43 ± 0.39 |
| A-Mem (re-run) | 39.79 ± 0.38 | 18.85 ± 0.31 | 54.05 ± 0.22 | 49.91 ± 0.31 |

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 1

### 2.5 Latency Analysis — Exact Numbers

From Table 2 and Section 4.4:

| Method | Search p50 | Search p95 | Total p50 | Total p95 |
|--------|-----------|-----------|----------|----------|
| **Mem0** | **0.148s** | **0.200s** | **0.708s** | **1.440s** |
| Mem0^g | 0.476s | 0.657s | 1.091s | 2.590s |
| OpenAI (ChatGPT) | — | — | 0.466s | 0.889s |
| Zep | 0.513s | 0.778s | 1.292s | 2.926s |
| A-Mem | 0.668s | 1.485s | 1.410s | 4.374s |
| **Full-context** | — | — | **9.870s** | **17.117s** |
| LangMem | 17.99s | 59.82s | 18.53s | 60.40s |

**Latency reduction:** Mem0 p95 (1.440s) vs Full-context p95 (17.117s):
- Absolute reduction: 17.117 - 1.440 = 15.677s
- Relative reduction: (17.117 - 1.440) / 17.117 × 100 = **91.6%** → paper states **"92% reduction"** (Section 4.3) / **"91% lower p95 latency"** (Abstract)

Mem0^g p95 (2.590s) vs Full-context: **85% reduction** per Section 4.3.

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 2, Sections 4.3–4.4

### 2.6 Token Consumption — Exact Numbers

From Table 2 ("chunk size / memory tokens" column) and Section 4.5:

| Method | Tokens per conversation |
|--------|------------------------|
| **Full-context (raw)** | **26,031** |
| Mem0^g | 3,616 |
| Zep | 3,911 |
| OpenAI (ChatGPT memory) | 4,437 |
| A-Mem | 2,520 |
| **Mem0** | **1,764** |
| LangMem | 127 |

From Section 4.5 running text:

> "Mem0 occupies only **~7k tokens** per conversation on average" (note: the ~7k appears to include extraction overhead; the Table 2 figure of 1,764 represents retrieved memory tokens used as answer context, while 7k includes the extraction LLM calls.)

> "Mem0^g roughly doubles the footprint to **~14k tokens**"

> "Supplying the entire raw conversation context to the language model—without any memory abstraction—amounts to roughly **26k tokens** on average"

**Token savings:** Mem0 (1,764 tokens) vs Full-context (26,031 tokens):
- 1 - (1,764 / 26,031) × 100 = **93.2% token reduction**
- Paper abstract: "**saves more than 90% token cost**"
- The mem0 research page states: "averaging under 7,000 tokens per retrieval call. Full-context approaches on the same benchmarks use 25,000+. High accuracy at **3-4x lower token cost**."

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 2, Section 4.5; [mem0.ai/research](https://mem0.ai/research)

### 2.7 mem0 vs mem0^g (Graph Variant)

| Metric | Mem0 | Mem0^g (graph) |
|--------|------|-----------------|
| Overall J Score | 66.88 ± 0.15 | **68.44 ± 0.17** (~2% higher) |
| Tokens per conversation | **1,764** | 3,616 (2.05× more) |
| Full token overhead | ~7k | ~14k |
| Total p50 latency | **0.708s** | 1.091s |
| Total p95 latency | **1.440s** | 2.590s |
| Search p50 | **0.148s** | 0.476s |
| Open-Domain J | 72.93 | **75.71** |
| Multi-Hop J | **51.15** | 47.19 |
| Temporal J | 55.51 | **58.13** |

Key trade-off: Mem0^g gets ~2% higher overall J score at roughly 2× the token cost and 1.8× the p95 latency.

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Tables 1–2, Sections 4.1–4.5

### 2.8 RAG Baseline Ablation (varying chunk sizes and k)

From Table 2 — Overall J scores for RAG configurations:

**k=1 (single most relevant chunk):**
| Chunk size | Overall J |
|------------|-----------|
| 128 | 47.77 ± 0.23 |
| 256 | 50.15 ± 0.16 |
| 512 | 46.05 ± 0.14 |
| 1024 | 40.74 ± 0.17 |
| 2048 | 37.93 ± 0.12 |
| 4096 | 36.84 ± 0.17 |
| 8192 | 44.53 ± 0.13 |

**k=2 (two most relevant chunks):**
| Chunk size | Overall J |
|------------|-----------|
| 128 | 59.56 ± 0.19 |
| 256 | **60.97 ± 0.20** (best RAG) |
| 512 | 58.19 ± 0.18 |
| 1024 | 50.68 ± 0.13 |
| 2048 | 48.57 ± 0.22 |
| 4096 | 51.79 ± 0.15 |
| 8192 | 60.53 ± 0.16 |

Best RAG (60.97%) vs Mem0 (66.88%) vs Full-context (72.90%).

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 2

### 2.9 Zep Graph Token Anomaly

An important note from Section 4.5:

> "Zep's memory graph consumes in excess of **600k tokens**" — this is 20× more than the raw conversation itself (26k tokens).

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Section 4.5

### 2.10 Memory Extraction + Retrieval Architecture

From Section 2.1:

- **Extraction phase**: Processes message pairs `(m_{t-1}, m_t)` + conversation summary `S` + `m=10` recent messages → LLM extracts salient memories `Ω = {ω_1, ..., ω_n}`
- **Update phase**: Each extracted fact compared against top `s=10` semantically similar existing memories → LLM decides ADD / UPDATE / DELETE / NOOP via function-calling
- **Inference engine**: GPT-4o-mini for both extraction and retrieval
- **Embedding model**: `text-embedding-3-small` (OpenAI)
- **Token encoding**: `cl100k_base` via tiktoken

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Section 2.1

**New algorithm (2025 README update):**
- Single-pass ADD-only extraction (one LLM call, no UPDATE/DELETE)
- Entity linking across memories
- Multi-signal retrieval: semantic + BM25 keyword + entity matching fused
- New scores: 91.6 on LoCoMo, 94.8 on LongMemEval, 64.1 on BEAM (1M), 48.6 on BEAM (10M)
- Mean tokens: ~7,000 per retrieval call across all benchmarks

Source: [https://github.com/mem0ai/mem0](https://github.com/mem0ai/mem0) (README, "New Memory Algorithm" section)

---

## 3. Cost Reduction Summary — Hierarchical Memory vs Full-Context

### 3.1 Token Reduction

| System | Full-context tokens | Memory tokens | Reduction |
|--------|-------------------|---------------|-----------|
| **mem0** | 26,031 avg per conversation | 1,764 retrieved tokens (or ~7k with extraction overhead) | **>90%** |
| **mem0^g** | 26,031 avg | 3,616 retrieved tokens (~14k total) | ~46-86% |
| **MemGPT** | Unbounded (would hit context limit) | Always ≤ base model context (8k tokens for GPT-4) | **No explicit % in paper** |

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 2, Section 4.5; [arXiv:2310.08560](https://arxiv.org/abs/2310.08560)

### 3.2 Latency Reduction

| System | p95 latency | vs Full-context (17.117s) |
|--------|-------------|---------------------------|
| **mem0** | 1.440s | **91% lower** |
| **mem0^g** | 2.590s | **85% lower** |
| **OpenAI (ChatGPT memory)** | 0.889s | 95% lower (but lower accuracy: 52.90 J) |
| **Zep** | 2.926s | 83% lower |
| **Full-context** | 17.117s | — |

Source: [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), Table 2, Sections 4.3–4.4

### 3.3 Dollar Cost

**Neither paper reports explicit dollar cost figures.** The mem0 paper discusses "token cost" only in relative/percentage terms ("saves more than 90% token cost"). No pricing-per-token calculations are provided for specific model endpoints.

For reference, using OpenAI GPT-4o-mini pricing ($0.15/1M input tokens, $0.60/1M output tokens as of mid-2025):
- Full-context: 26,031 tokens/call → ~$0.004 per query (input only)
- mem0: ~7,000 tokens/call → ~$0.001 per query

These are approximate and NOT from the papers. The actual cost depends on model choice, extraction overhead, and retrieval chaining.

### 3.4 The Architecture Pattern

Both systems share the same fundamental insight: **keep working memory small, archive everything, retrieve only what's relevant.**

```
┌───────────────┐     ┌──────────────────┐     ┌────────────────────┐
│  Main Context │←───→│ Retrieval Layer   │←───→│ External Storage    │
│  (small, fast)│     │ (function calls,  │     │ (vector DB, graph,  │
│               │     │  embeddings)      │     │  archival, files)   │
│  ~1k-8k tokens│     │                   │     │ Unlimited           │
└───────────────┘     └──────────────────┘     └────────────────────┘
```

- **MemGPT/Letta**: LLM self-manages via function calls; hierarchical tiers with urgency signals (memory pressure, queue eviction); git-backed memory filesystem (MemFS).
- **mem0**: External management via extraction→update→retrieval pipeline; ADD/UPDATE/DELETE/NOOP operations; graph variant adds entity-relationship modeling.

---

## 4. Key Takeaways

| Metric | mem0 | MemGPT/Letta |
|--------|------|--------------|
| **Token reduction vs full-context** | **>90%** (paper) | Not quantified in paper |
| **Latency reduction p95** | **91%** (1.44s vs 17.12s) | Not quantified in paper |
| **Accuracy impact (vs base model)** | 26% relative improvement over OpenAI memory (66.88 J vs 52.90 J) | GPT-4: 32.1% → 92.5% accuracy on DMR task |
| **Full-context outperforms on accuracy?** | Yes (72.90 J for full-context, but at 12× higher latency and 15× more tokens) | Full-context cannot complete tasks that exceed window; MemGPT can |
| **Graph variant benefit** | ~2% higher overall J, 2× token cost | Not applicable (graph memory not in original MemGPT) |
| **Dollar cost figures** | Not reported | Not reported |

---

## 5. Source Index

| Source | URL |
|--------|-----|
| MemGPT paper | [https://arxiv.org/abs/2310.08560](https://arxiv.org/abs/2310.08560) |
| MemGPT research page | [https://research.memgpt.ai](https://research.memgpt.ai) |
| Letta GitHub | [https://github.com/letta-ai/letta](https://github.com/letta-ai/letta) |
| Letta context hierarchy docs | [https://docs.letta.com/guides/core-concepts/memory/context-hierarchy](https://docs.letta.com/guides/core-concepts/memory/context-hierarchy) |
| Letta memory blocks docs | [https://docs.letta.com/guides/core-concepts/memory/memory-blocks](https://docs.letta.com/guides/core-concepts/memory/memory-blocks) |
| Letta compaction docs | [https://docs.letta.com/guides/core-concepts/messages/compaction](https://docs.letta.com/guides/core-concepts/messages/compaction) |
| Letta Code memory docs | [https://docs.letta.com/letta-code/memory](https://docs.letta.com/letta-code/memory) |
| mem0 paper | [https://arxiv.org/abs/2504.19413](https://arxiv.org/abs/2504.19413) |
| mem0 paper (HTML) | [https://arxiv.org/html/2504.19413v1](https://arxiv.org/html/2504.19413v1) |
| mem0 GitHub | [https://github.com/mem0ai/mem0](https://github.com/mem0ai/mem0) |
| mem0 research page | [https://mem0.ai/research](https://mem0.ai/research) |
| mem0 eval framework | [https://github.com/mem0ai/memory-benchmarks](https://github.com/mem0ai/memory-benchmarks) |
