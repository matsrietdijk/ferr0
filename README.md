# ferr0

A Rust CLI for self-hosted Mem0.

The name combines ferrum (iron), Rust’s Ferris, and Mem0’s zero.

## Status

Early development. ferr0 connects to an existing self-hosted Mem0 REST server using its API key.

## Usage

```sh
ferr0 config set url http://localhost:8888
ferr0 config set api-key <key>
ferr0 config set user-id alice

ferr0 add "Prefers green tea"
ferr0 search "drinks" --limit 5
ferr0 list
ferr0 update <memory-id> "Prefers black coffee"
ferr0 delete <memory-id>
```

Settings resolve in order: flags (`--url`, `--api-key`, `--user-id`, `--agent-id`, `--run-id`), then `FERR0_*` environment variables, then `$XDG_CONFIG_HOME/ferr0/config.toml` (default `~/.config`). Add `--json` to print the raw server response.

## Agent skill

[`skills/ferr0/SKILL.md`](skills/ferr0/SKILL.md) teaches AI coding agents to use ferr0. It follows the [Agent Skills](https://agentskills.io) format, so any compatible agent can use it: copy the `skills/ferr0` directory into your agent's skills directory.

## Development

```sh
cargo run
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```
