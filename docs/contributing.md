# Contributing

## Setup

```bash
cargo build
cargo test
cargo run -- chat
```

## Project Structure

See [docs/architecture.md](architecture.md) for the crate layout and data flow.

## Commit Convention

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add ast-grep structural search tool
fix: prevent tool call format mismatch in DeepSeek provider
docs: update README with model list
test: add config loading unit tests
refactor: extract retry logic into separate module
chore: update dependencies
```

## Adding a Provider

1. Create `crates/ophis-llm/src/providers/<name>.rs`
2. Implement `LlmProvider` trait
3. Register in `crates/ophis-llm/src/providers/mod.rs`
4. Add to `Config::default()` in `crates/ophis-core/src/config.rs`
5. Add to `Agent::new()` provider registry in `crates/ophis-core/src/agent.rs`

## Adding a Tool

1. Create `crates/ophis-tools/src/<name>.rs`
2. Implement `Tool` trait (definition + execute)
3. Register in `ToolRegistry::register_builtins()`

## Running Tests

```bash
cargo test
cargo test --package ophis-tools
cargo test --package ophis-core
```
