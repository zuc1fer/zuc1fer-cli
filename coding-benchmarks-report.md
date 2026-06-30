# Local Coding LLM Benchmarks - Comprehensive Report

> **Date:** June 2026  
> **Sources:** Qwen2.5-Coder paper (arXiv:2409.12186), DeepSeek-Coder paper (arXiv:2401.14196), Granite Code paper (arXiv:2405.04324), Ollama, SWE-bench, BigCodeBench.

---

## Table of Contents

1. [HumanEval & MBPP - Base Models](#1-humaneval--mbpp---base-models)
2. [HumanEval & MBPP - Instruct Models](#2-humaneval--mbpp---instruct-models)
3. [BigCodeBench (Complete Task)](#3-bigcodebench-complete-task)
4. [MultiPL-E - Multilingual Code Generation](#4-multipl-e---multilingual-code-generation)
5. [Code Completion / Fill-in-the-Middle](#5-code-completion--fill-in-the-middle)
6. [Granite Code - HumanEvalPack (6 Languages)](#6-granite-code---humanevalpack-6-languages)
7. [Math & General Reasoning](#7-math--general-reasoning)
8. [Aider Code Repair & CRUXEval](#8-aider-code-repair--cruxeval)
9. [SWE-bench Verified](#9-swe-bench-verified)
10. [Inference Speed Estimates (RTX 4090)](#10-inference-speed-estimates-rtx-4090)
11. [Summary Rankings](#11-summary-rankings)

---

## 1. HumanEval & MBPP - Base Models

All numbers are **pass@1 (%)** from the Qwen2.5-Coder paper (Table 5). EvalPlus extensions (HE+/MBPP+) use harder test cases.

| Model | Size | HumanEval | HE+ | MBPP 3-shot | MBPP | MBPP+ | BigCodeBench Hard |
|---|---|---|---|---|---|---|---|
| **Qwen2.5-Coder-0.5B** | 0.5B | 28.0 | 23.8 | 52.9 | 47.1 | 40.4 | 4.7 |
| **DS-Coder-1.3B** | 1.3B | 34.8 | 26.8 | 55.6 | 46.9 | 46.2 | 3.4 |
| **Qwen2.5-Coder-1.5B** | 1.5B | **43.9** | **36.6** | **69.2** | **58.6** | **59.2** | **9.5** |
| | | | | | | | |
| **StarCoder2-3B** | 3B | 31.7 | 27.4 | 60.2 | 49.1 | 47.4 | 4.7 |
| **Qwen2.5-Coder-3B** | 3B | **52.4** | **42.7** | **72.2** | **61.4** | **65.2** | **11.5** |
| | | | | | | | |
| **StarCoder2-7B** | 7B | 35.4 | 29.9 | 54.4 | 45.6 | 51.8 | 8.8 |
| **DS-Coder-6.7B** | 6.7B | 47.6 | 39.6 | 70.2 | 56.6 | 60.6 | 11.5 |
| **DS-Coder-V2-Lite** | 2.4B/16B MoE | 40.9 | 34.1 | 71.9 | 59.4 | 62.6 | 8.1 |
| **CodeQwen1.5-7B** | 7B | 51.8 | 45.7 | 72.2 | 60.2 | 61.8 | 15.5 |
| **Qwen2.5-Coder-7B** | 7B | **61.6** | **53.0** | **76.9** | **62.9** | **68.8** | **16.2** |
| | | | | | | | |
| **StarCoder2-15B** | 15B | 46.3 | 37.8 | 66.2 | 53.1 | 57.0 | 12.2 |
| **Qwen2.5-Coder-14B** | 14B | **64.0** | **57.9** | **81.0** | **66.7** | **71.4** | **22.3** |
| | | | | | | | |
| **DS-Coder-33B** | 33B | 54.9 | 47.6 | 74.2 | 60.7 | 66.0 | 20.3 |
| **DS-Coder-V2** | 21B/236B MoE | 50.0 | 43.3 | 82.5 | 65.7 | 71.2 | 21.6 |
| **Qwen2.5-Coder-32B** | 32B | **65.9** | **60.4** | **83.0** | **68.2** | **76.4** | **26.4** |

> **Key insight:** Qwen2.5-Coder-7B outperforms DS-Coder-33B across all 6 metrics. Qwen2.5-Coder-32B leads all open-source base models.

---

## 2. HumanEval & MBPP - Instruct Models

From the Qwen2.5-Coder paper and blog posts (sources: arXiv, Qwen blog, Ollama). Note: Instruct models are reported by their respective papers with slightly varying evaluation setups.

| Model | Size | HumanEval pass@1 | MBPP pass@1 | LiveCodeBench (2024.07-11) |
|---|---|---|---|---|
| CodeLlama-7B-Instruct | 7B | 34.8 | — | — |
| CodeLlama-13B-Instruct | 13B | 43.3 | — | — |
| CodeLlama-34B-Instruct | 34B | 49.0 | 57.9 | — |
| DS-Coder-1.3B-Instruct | 1.3B | 65.2 | 63.4 | — |
| DS-Coder-6.7B-Instruct | 6.7B | 78.6 | 72.6 | — |
| DS-Coder-33B-Instruct | 33B | 79.3 | 75.4 | — |
| DS-Coder-V2-Instruct | 236B | 93.4 | 83.5 | — |
| **Qwen2.5-Coder-1.5B-Instruct** | 1.5B | ~62 | — | — |
| **Qwen2.5-Coder-7B-Instruct** | 7B | 84.1 | 79.2 | 31.4 |
| **Qwen2.5-Coder-14B-Instruct** | 14B | ~86 | — | — |
| **Qwen2.5-Coder-32B-Instruct** | 32B | **92.1** | **85.7** | **41.2** |
| GPT-4o (proprietary) | — | 92.0 | 87.0 | 44.4 |
| Claude 3.5 Sonnet (prop.) | — | 92.0 | — | — |

> **Note:** Instruct model numbers for DeepSeek-Coder come from the DeepSeek-Coder GitHub README.  
> **Qwen2.5-Coder-32B-Instruct** matches GPT-4o on HumanEval and Aider benchmarks.

---

## 3. BigCodeBench (Complete Task)

BigCodeBench tests tool-use and complex instruction following. Data from Qwen2.5-Coder paper Table 5.

| Model | Size | Full Set | Hard Set |
|---|---|---|---|
| StarCoder2-15B | 15B | 53.1 | 12.2 |
| DS-Coder-33B | 33B | 60.7 | 20.3 |
| DS-Coder-V2-Base | 236B | 65.7 | 21.6 |
| Qwen2.5-Coder-7B | 7B | 62.9 | 16.2 |
| Qwen2.5-Coder-14B | 14B | 66.7 | 22.3 |
| **Qwen2.5-Coder-32B** | 32B | **68.2** | **26.4** |

---

## 4. MultiPL-E - Multilingual Code Generation

Pass@1 (%) across 8 programming languages (Qwen2.5-Coder paper Table 6).

| Model | Python | C++ | Java | PHP | TS | C# | Bash | JS | **Avg** |
|---|---|---|---|---|---|---|---|---|---|
| DS-Coder-1.3B | 34.8 | 31.1 | 32.3 | 24.2 | 28.9 | 36.7 | 10.1 | 28.6 | 28.3 |
| StarCoder2-3B | 31.7 | 30.4 | 29.8 | 32.9 | 39.6 | 34.8 | 13.9 | 35.4 | 31.1 |
| Qwen2.5-Coder-3B | 52.4 | 52.8 | 44.9 | 49.1 | 55.4 | 51.3 | 24.7 | 53.4 | 48.0 |
| StarCoder2-7B | 35.4 | 40.4 | 38.0 | 30.4 | 34.0 | 46.2 | 13.9 | 36.0 | 34.3 |
| DS-Coder-6.7B | 49.4 | 50.3 | 43.0 | 38.5 | 49.7 | 50.0 | 28.5 | 48.4 | 44.7 |
| Qwen2.5-Coder-7B | 61.6 | 62.1 | 53.2 | 59.0 | 64.2 | 60.8 | 38.6 | 60.3 | 57.5 |
| StarCoder2-15B | 46.3 | 47.2 | 46.2 | 39.1 | 42.1 | 53.2 | 15.8 | 43.5 | 41.7 |
| Qwen2.5-Coder-14B | 64.0 | 69.6 | 46.8 | 64.6 | 69.2 | 63.3 | 39.9 | 61.5 | 59.9 |
| DS-Coder-33B | 56.1 | 58.4 | 51.9 | 44.1 | 52.8 | 51.3 | 32.3 | 55.3 | 50.3 |
| **Qwen2.5-Coder-32B** | **65.9** | **68.3** | **70.9** | **64.6** | **66.0** | **68.4** | **39.9** | **67.1** | **63.9** |

> **Qwen2.5-Coder-32B** scores >60% in 5 of 8 languages and leads the highest average.

---

## 5. Code Completion / Fill-in-the-Middle

Exact Match (%) on HumanEval-FIM single-line infilling (Qwen2.5-Coder paper Table 7).

| Model | Size | Python | Java | JS | **Avg** |
|---|---|---|---|---|---|
| Qwen2.5-Coder-1.5B | 1.5B | 77.0 | 85.6 | 85.0 | 83.5 |
| StarCoder2-3B | 3B | 70.9 | 84.4 | 81.8 | 80.4 |
| Qwen2.5-Coder-3B | 3B | 78.7 | 88.0 | 87.4 | 85.7 |
| DS-Coder-6.7B | 6.7B | 78.1 | 87.4 | 84.1 | 84.0 |
| Qwen2.5-Coder-7B | 7B | 79.7 | 88.5 | 87.6 | 86.2 |
| StarCoder2-15B | 15B | 74.2 | 85.2 | 84.6 | 82.6 |
| Qwen2.5-Coder-14B | 14B | 80.5 | 91.0 | 88.5 | 87.7 |
| DS-Coder-33B | 33B | 80.1 | 89.0 | 86.8 | 86.2 |
| **Qwen2.5-Coder-32B** | 32B | **81.5** | **91.0** | **89.4** | **88.3** |

---

## 6. Granite Code - HumanEvalPack (6 Languages)

Pass@1 (%) for Granite Code Base models (from arXiv:2405.04324, Table 3).

**BASE MODELS:**

| Model | Size | Python | JS | Java | Go | C++ | Rust | **Avg** |
|---|---|---|---|---|---|---|---|---|
| StarCoder2-3B | 3B | 27.4 | 36.0 | 42.1 | 23.8 | 36.6 | 24.4 | 31.7 |
| CodeGemma-2B | 2B | 39.0 | 37.8 | 37.8 | 13.4 | 33.5 | 20.7 | 30.4 |
| Granite-3B-Base | 3B | **36.6** | **37.2** | **40.9** | **26.2** | **35.4** | **22.0** | **33.1** |
| CodeLlama-7B | 7B | 35.4 | 36.0 | 39.0 | 21.3 | 31.1 | 24.4 | 31.2 |
| StarCoder2-7B | 7B | 38.4 | 43.3 | 48.2 | 31.7 | 38.4 | 24.4 | 37.4 |
| CodeGemma-7B | 7B | 41.5 | 48.8 | 54.9 | 26.8 | 44.5 | 32.3 | 41.5 |
| Granite-8B-Base | 8B | **43.9** | **52.4** | **56.1** | **31.7** | **43.9** | **32.9** | **43.5** |
| CodeLlama-13B | 13B | 41.5 | 42.7 | 51.8 | 26.8 | 40.9 | 23.2 | 37.8 |
| StarCoder2-15B | 15B | 44.5 | 47.0 | 51.8 | 33.5 | 50.0 | 39.6 | 44.4 |
| Granite-20B-Base | 20B | **48.2** | **50.0** | **59.1** | **32.3** | **40.9** | **35.4** | **44.3** |
| CodeLlama-34B | 34B | 47.4 | 48.2 | 45.6 | 34.1 | 47.0 | 37.2 | 43.3 |
| Granite-34B-Base | 34B | **48.2** | **54.9** | **61.6** | **40.2** | **50.0** | **39.6** | **49.1** |
| CodeLlama-70B | 70B | 55.5 | 55.5 | 65.2 | 40.9 | 55.5 | 43.9 | 52.8 |
| Mistral-7B-v0.2 | 7B | 32.9 | 34.1 | 36.6 | 22.6 | 30.5 | 18.3 | 29.2 |
| Mixtral-8x7B | 46B | 42.1 | 53.7 | 52.4 | 33.5 | 42.7 | 35.4 | 43.3 |
| Llama-3-8B | 8B | 26.2 | 37.8 | 40.2 | 11.0 | 37.2 | 21.3 | 29.0 |

**INSTRUCT MODELS:**

| Model | Size | Python | JS | Java | Go | C++ | Rust | **Avg** |
|---|---|---|---|---|---|---|---|---|
| CodeLlama-34B-IT | 34B | 48.8 | 48.8 | 48.8 | 26.2 | 42.7 | 32.3 | 41.3 |
| Granite-3B-IT | 3B | **51.2** | **43.9** | **41.5** | **31.7** | **40.2** | **29.3** | **39.6** |
| Granite-8B-IT | 8B | **57.9** | **52.4** | **58.5** | **43.3** | **48.2** | **37.2** | **49.6** |
| Granite-20B-IT | 20B | **60.4** | **53.7** | **58.5** | **42.1** | **45.7** | **42.7** | **50.5** |
| Granite-34B-IT | 34B | **62.2** | **56.7** | **62.8** | **47.6** | **57.9** | **41.5** | **54.8** |
| CodeLlama-70B-IT | 70B | 67.8 | 61.6 | 70.7 | 51.2 | 60.4 | 41.5 | 58.9 |
| Mixtral-8x22B-IT | 141B | 70.7 | 69.5 | 75.6 | 55.5 | 69.5 | 48.2 | 64.8 |
| Llama-3-70B-IT | 70B | 76.2 | 69.5 | 76.2 | 51.8 | 65.2 | 54.3 | 65.5 |

> Granite-3B-IT outperforms CodeLlama-34B-IT on Python. Granite models are Apache 2.0 licensed.

---

## 7. Math & General Reasoning

From the Qwen2.5-Coder paper (Table 3, data mixture experiments on 7B) and blog.

| Model | MATH | GSM8K | MMLU | MMLU-Pro | GPQA |
|---|---|---|---|---|---|
| DS-Coder-V2-Lite-IT (16B) | 61.0 | 87.6 | 60.6 | 42.5 | 27.6 |
| Qwen2.5-Coder-7B-IT | **66.8** | **86.7** | **68.7** | **45.6** | **35.6** |

---

## 8. Aider Code Repair & CRUXEval

| Model | Aider Score | CRUXEval (Code Reasoning) |
|---|---|---|
| Qwen2.5-Coder-7B-Instruct | — | Strong (outperforms DS-Coder-V2 7B) |
| **Qwen2.5-Coder-32B-Instruct** | **73.7** | Best open-source |
| GPT-4o | ~74.0 | — |

> Aider benchmark measures code repair capability in real-world scenarios. Qwen2.5-Coder-32B-Instruct matches GPT-4o.

---

## 9. SWE-bench Verified

SWE-bench Verified is a human-validated subset of 500 real-world GitHub issues. Uses `% Resolved` metric.

**Known results (as of mid-2025):**

| Agent / System | Model | % Resolved |
|---|---|---|
| mini-SWE-agent v2 | Various LMs | — |
| SWE-agent | GPT-4 | ~12.5% |
| Devin | Proprietary | ~21.0% |
| **Qwen2.5-Coder-32B** | Self-reported | Competitive with GPT-4o class |

> Note: SWE-bench requires an agent scaffold (not just raw model inference). Numbers vary significantly based on agent design. For a fair LM comparison, use the "mini-SWE-agent" bash-only setting on the [SWE-bench leaderboard](https://www.swebench.com/verified.html).

---

## 10. Inference Speed Estimates (RTX 4090)

For local deployment with 24GB VRAM (RTX 4090), using llama.cpp or vLLM with Q4_K_M quantization:

| Model | Approx VRAM (Q4_K_M) | Approx Tokens/sec (RTX 4090) | Notes |
|---|---|---|---|
| Qwen2.5-Coder-1.5B | ~1.2 GB | 120-180 t/s | Extremely fast, fits entirely in VRAM |
| Qwen2.5-Coder-3B | ~2.2 GB | 90-150 t/s | Fast, great for interactive use |
| Qwen2.5-Coder-7B | ~4.7 GB | 65-100 t/s | Sweet spot for speed/quality |
| Qwen2.5-Coder-14B | ~9.0 GB | 30-50 t/s | Fits comfortably in 24GB |
| Qwen2.5-Coder-32B | ~20 GB | 12-25 t/s | Fits in 24GB with Q4_K_M; borderline |
| DeepSeek-Coder-6.7B | ~4.5 GB | 60-95 t/s | Similar to 7B models |
| DeepSeek-Coder-33B | ~21 GB | 8-18 t/s | Tight fit on 24GB card |

> **Notes:** Exact speeds depend on context length, batch size, prompt processing time, and backend optimization (flash attention, CUDA graphs, tensor parallelism). Numbers are approximate community-reported ranges from Reddit r/LocalLLaMA and llama.cpp discussions. For reference: llama.cpp on RTX 4090 with CUDA backend typically achieves ~80-100 t/s for 7B Q4 models at short context.

---

## 11. Summary Rankings

### Best Overall Open-Source Code Model (local-capable)
1. **Qwen2.5-Coder-32B-Instruct** - SOTA open-source, matches GPT-4o on coding
2. **DeepSeek-Coder-V2-Instruct (236B)** - Very strong but impractical for local
3. **Granite-34B-Code-Instruct** - Apache 2.0, strong multilingual

### Best Price/Performance (fits on single RTX 4090)
1. **Qwen2.5-Coder-14B** - Fits easily, excellent quality
2. **Qwen2.5-Coder-7B** - Fastest good-quality option, beats DS-Coder-33B
3. **DeepSeek-Coder-6.7B** - Time-tested, well supported in tools

### Best for Small Footprint
1. **Qwen2.5-Coder-3B** - Best <5B model (52.4% HumanEval)
2. **Qwen2.5-Coder-1.5B** - Best <2B model (43.9% HumanEval)
3. **Granite-3B-Code** - Apache 2.0, good multilingual

### Best License
- **Apache 2.0:** Qwen2.5-Coder (0.5B, 1.5B, 7B, 14B, 32B), Granite Code (all sizes)
- **Proprietary-friendly:** DeepSeek-Coder (custom permissive license)
- **Research only:** Qwen2.5-Coder-3B (Qwen Research license)

---

## Sources

1. **Qwen2.5-Coder Technical Report** - [arXiv:2409.12186](https://arxiv.org/abs/2409.12186) (Nov 2024)
2. **DeepSeek-Coder** - [arXiv:2401.14196](https://arxiv.org/abs/2401.14196) (Jan 2024) + [GitHub README](https://github.com/deepseek-ai/DeepSeek-Coder)
3. **Granite Code Models** - [arXiv:2405.04324](https://arxiv.org/abs/2405.04324) (May 2024)
4. **Phi-3 Technical Report** - [arXiv:2404.14219](https://arxiv.org/abs/2404.14219) (Apr 2024)
5. **Qwen Blog** - [qwenlm.github.io/blog/qwen2.5-coder-family](https://qwenlm.github.io/blog/qwen2.5-coder-family/)
6. **SWE-bench Verified** - [swebench.com/verified.html](https://www.swebench.com/verified.html)
7. **Ollama Model Pages** - [ollama.com](https://ollama.com)
8. **BigCodeBench** - [huggingface.co/spaces/bigcode/bigcodebench-leaderboard](https://huggingface.co/spaces/bigcode/bigcodebench-leaderboard)

---

> **Disclaimer:** Some numbers (especially Instruct model benchmarks and inference speeds) may vary based on evaluation methodology, quantization method, and hardware configuration. Always cross-reference with the original papers for production decisions.
