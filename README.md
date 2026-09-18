# ferr0

A Rust CLI for self-hosted Mem0.

The name combines ferrum (iron), Rust’s Ferris, and Mem0’s zero.

## Status

Early development. ferr0 connects to an existing self-hosted Mem0 REST server using its API key.

## Usage

Run `ferr0 setup` to enter the server URL, API key, and user id, check the connection, and install the agent skill. Or set values directly:

```sh
ferr0 config set url http://localhost:8888
ferr0 config set api-key <key>
ferr0 config set user-id alice
ferr0 config set agent-id my-agent
ferr0 config get user-id
ferr0 config show

ferr0 add "Prefers green tea"
echo "Works from home on Fridays" | ferr0 add
ferr0 add --messages '[{"role": "user", "content": "Use pnpm"}, {"role": "assistant", "content": "Switched the repo to pnpm"}]'
ferr0 add --file messages.json
ferr0 search "drinks" --limit 5
ferr0 list
ferr0 get <memory-id>
ferr0 update <memory-id> "Prefers black coffee"
ferr0 delete <memory-id>
```

Settings resolve in order: flags (`--url`, `--api-key`, `--user-id`, `--agent-id`, `--run-id`), then `FERR0_*` environment variables, then `$XDG_CONFIG_HOME/ferr0/config.toml` (default `~/.config`). Like the official Mem0 CLI, any scope flag (`--user-id`, `--agent-id`, `--run-id`) replaces all scope ids from the environment and config file; without scope flags, each id falls back separately. `config get` prints one value from its `FERR0_*` environment variable or the config file, with the API key redacted, and an empty line when the value is unset. Add `--json` to print the raw server response, or on failure `{"error": {"message": "...", "status": 401}}` on stdout, where `status` is only present for server errors.

## Agent skill

[`skills/ferr0/SKILL.md`](skills/ferr0/SKILL.md) teaches AI coding agents to use ferr0. It follows the [Agent Skills](https://agentskills.io) format, so any compatible agent can use it. `ferr0 setup` installs it into `~/.agents/skills` (read by most agents), `~/.claude/skills`, or an agent's own skills directory, or you can copy the `skills/ferr0` directory yourself.

## Development

```sh
cargo run
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```
