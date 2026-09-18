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

ferr0 add "Prefers green tea"
echo "Works from home on Fridays" | ferr0 add
ferr0 add --messages '[{"role": "user", "content": "Use pnpm"}, {"role": "assistant", "content": "Switched the repo to pnpm"}]'
ferr0 add --file messages.json
ferr0 add "Standup moves to 10:00" --metadata '{"topic": "work"}' --expires 2030-12-31
ferr0 add --no-infer "Deploys happen on Tuesdays"
ferr0 search "drinks" --limit 5
ferr0 list
ferr0 get <memory-id>
ferr0 update <memory-id> "Prefers black coffee"
ferr0 update <memory-id> --metadata '{"topic": "drinks"}' --no-expires
ferr0 delete <memory-id>
ferr0 import memories.json
```

`import` adds each item of a JSON array with its own request. Items are strings or `{"memory": "...", "metadata": {...}, "user_id": "...", "agent_id": "..."}` objects, where `text` or `content` may replace `memory`. The resolved user and agent ids take precedence over an item's own. A failed item is counted and does not stop the import; `--json` prints each item's result.

Settings resolve in order: flags (`--url`, `--api-key`, `--user-id`, `--agent-id`, `--run-id`), then `FERR0_*` environment variables, then `$XDG_CONFIG_HOME/ferr0/config.toml` (default `~/.config`). Add `--json` to print the raw server response, or on failure `{"error": {"message": "...", "status": 401}}` on stdout, where `status` is only present for server errors.

## Agent skill

[`skills/ferr0/SKILL.md`](skills/ferr0/SKILL.md) teaches AI coding agents to use ferr0. It follows the [Agent Skills](https://agentskills.io) format, so any compatible agent can use it. `ferr0 setup` installs it into `~/.agents/skills` (read by most agents), `~/.claude/skills`, or an agent's own skills directory, or you can copy the `skills/ferr0` directory yourself.

## Development

```sh
cargo run
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```
