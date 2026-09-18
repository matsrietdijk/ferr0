---
name: ferr0
description: Store, search, list, update, and delete long-term memories in a self-hosted Mem0 server through the ferr0 CLI. Use only when the user's prompt explicitly refers to memories, for example "remember that...", "what do you remember about...", "forget...", or mentions ferr0 or Mem0. Do not use it to recall or store context on your own initiative.
compatibility: Requires the ferr0 CLI and network access to a self-hosted Mem0 server.
---

# ferr0

`ferr0` is a command-line client for a self-hosted Mem0 REST server. Use it only when the user asks you to work with memories.

## Setup

- If the `ferr0` command is not found, tell the user to install ferr0 and stop.
- If configuration is missing, ask the user to set it with `ferr0 config set <url|api-key|user-id> <value>` and stop. Never ask for the API key in chat, never print it, and never pass it with `--api-key`, because command lines end up in logs and transcripts.

## Scope

Memories are scoped by user and agent, and every id passed to `search` or `list` must match.

- **User:** use the configured user id and never invent one. Pass `--user-id` only when the user explicitly asks to work with a specific other user id, and only for that request.
- **Agent:** identify yourself with a stable agent id, derived from the agent application you run in, not the model or vendor. Use lowercase kebab-case, such as `claude-code`, `codex-cli`, `gemini-cli`, `cursor`, `github-copilot`, or `opencode`. Never include model names, versions, or session ids. If you cannot tell which application you run in, ask the user once.
- Pass `--agent-id <id>` on every `add`, so each memory records which agent stored it.
- Do not pass `--agent-id` to `search` or `list`, so memories stored by other agents for the same user are found. Only pass it when the user asks for memories from this agent. If `FERR0_AGENT_ID` is set in the environment, run these as `env -u FERR0_AGENT_ID ferr0 ...` to avoid narrowing the results.
- Do not pass `--run-id`; memories should outlive the session.

## Commands

Always add `--json` and read the response as JSON.

| Goal | Command |
| --- | --- |
| Store a fact | `ferr0 --json add --agent-id <id> "<text>"` |
| Store messages with roles | `ferr0 --json add --agent-id <id> --messages '<json>'` |
| Find relevant memories | `ferr0 --json search "<query>" [--limit N] [--show-expired]` |
| List stored memories | `ferr0 --json list [--limit N] [--show-expired]` |
| Show one memory | `ferr0 --json get <memory-id>` |
| Replace a memory's text | `ferr0 --json update <memory-id> "<text>"` |
| Delete a memory | `ferr0 --json delete <memory-id>` |

`add` and `search` read the text or query from stdin when it is omitted and input is piped, for example `printf '%s' "<text>" | ferr0 --json add --agent-id <id>`, which avoids shell quoting problems.

Text passed to `add` is stored as a message from the user. To record what you said or did, pass `--messages` with a JSON array of `{"role": "...", "content": "..."}` objects, or pipe the array with `--file /dev/stdin`:

```sh
printf '%s' '[{"role": "user", "content": "<request>"}, {"role": "assistant", "content": "<what you did>"}]' \
  | ferr0 --json add --agent-id <id> --file /dev/stdin
```

- `user`: something the user stated, such as a preference, plan, or fact about themselves.
- `assistant`: something you did or said, such as a recommendation, a decision, or information you researched.
- Include the user message the assistant message responds to when the assistant message does not stand on its own.

`add`, `search`, and `list` return `{"results": [...]}`. Each result has an `id` and `memory`, and stored memories have an `attributed_to` of `user` or `assistant` that tells whose statement the memory came from; search results also have a `score`, and add results have an `event` (`ADD`, `UPDATE`, `DELETE`, or `NONE`). `get` returns a single memory object with `id` and `memory`. `update` and `delete` return `{"message": "..."}`.

## Working with memories

- When recalling, use only results that are relevant to the user's question.
- Store short, self-contained facts in plain language, for example `Prefers pnpm over npm`. The server extracts and deduplicates memories itself, so an add can return `UPDATE` or `NONE` instead of `ADD`, and may take a few seconds.
- Do not store secrets, credentials, or sensitive personal data unless the user explicitly asks.
- Only `update` or `delete` memory ids returned by `search` or `list`, with the same user id, so every change stays within that user. The memory may have been stored by any agent.
- Deletion cannot be undone. Confirm with the user before deleting unless they asked for that specific deletion.

## Failures

With `--json`, ferr0 exits with a non-zero status and prints the error as JSON on stdout: `{"error": {"message": "...", "status": 401}}`. `status` is the HTTP status code and is present only when the server returned an error. Match the cases below against `message`.

- `no server URL`: the server is not configured; ask the user to run `ferr0 config set url <url>`.
- `cannot reach the Mem0 server`: the server is down or the URL is wrong; report it rather than retrying repeatedly.
- `server returned 401` or `403`: the API key is missing or invalid; ask the user to fix it.
- `server returned 400` or `422`: the request is invalid; read the detail and report it if you cannot correct your own command.
