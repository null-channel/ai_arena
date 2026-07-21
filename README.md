# AI Arena

AI Arena is a Rust harness for pitting language models against one another in small, structured games. It gives each model the same JSON game state and move schema, validates every response against the game rules, and records enough detail to compare behavior, speed, reliability, and token use.

The project currently supports OpenAI, Anthropic, and Ollama agents playing Tic-Tac-Toe, Connect Four, and Rock-Paper-Scissors.

## What it provides

- Manual matches and repeatable CSV batches.
- Configurable model, temperature, seed, credentials profile, timeout, retry, concurrency, and token-budget settings.
- Fairer repeated trials by reversing agent seats on every even-numbered match.
- Explicit win, draw, forfeit, and infrastructure-error outcomes.
- Human-readable summaries plus versioned JSON Lines output for downstream analysis.
- A public Rust library, a thin CLI, deterministic game tests, and strict CI quality gates.

## Quick start

The repository pins Rust 1.97.1 in `rust-toolchain.toml`. Install [rustup](https://rustup.rs/) and a system C linker, then build:

```bash
git clone https://github.com/null-channel/ai_arena.git
cd ai_arena
cargo build --release --locked
```

Configure the providers used by your match:

```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
export OLLAMA_BASE_URL="http://localhost:11434"
```

`OLLAMA_BASE_URL` is optional and defaults to `http://localhost:11434`. Model names always come from the CLI or CSV; there is no `OLLAMA_MODEL` environment setting.

Run a match:

```bash
cargo run --release --locked -- \
  --game-name TicTacToe \
  --agent-one-kind OpenAI \
  --agent-one-model gpt-4o-mini \
  --agent-one-temp 0.2 \
  --agent-one-seed 42 \
  --agent-two-kind Ollama \
  --agent-two-model llama3.2 \
  --agent-two-temp 0.2 \
  --agent-two-seed 43 \
  --repetitions 10 \
  --output-jsonl results.jsonl
```

Use `cargo run --release -- --help` to see every option. If the release binary has been installed or copied onto your `PATH`, replace `cargo run --release --locked --` with `ai_arena`.

## Credentials and provider profiles

Environment variables are enough for a single credential per provider. Named profiles make it possible to compare accounts or Ollama endpoints without putting secrets in a shareable CSV.

Copy [`examples/secrets.toml.example`](examples/secrets.toml.example) to one of these locations:

- `$XDG_CONFIG_HOME/ai_arena/secrets.toml` when `XDG_CONFIG_HOME` is set.
- `~/.config/ai_arena/secrets.toml` otherwise.

Protect the file before adding real keys:

```bash
chmod 600 ~/.config/ai_arena/secrets.toml
```

Then select profiles with `--agent-one-secret-profile` and `--agent-two-secret-profile`, or with the equivalent CSV columns. Resolution order is:

1. The requested named profile.
2. The provider environment variable.
3. The provider's `default` profile.
4. An actionable error, except Ollama, which falls back to its localhost URL.

OpenAI uses `OPENAI_API_KEY`, Anthropic uses `ANTHROPIC_API_KEY`, and Ollama uses `OLLAMA_BASE_URL`.

## Games and match behavior

| Game | Default configuration | Turn behavior |
| --- | --- | --- |
| `TicTacToe` | 3×3 board, three in a row | Agents alternate; an invalid move leaves the board and player unchanged. |
| `ConnectFour` | 6×7 board, four in a row | Agents alternate; an invalid move leaves the board and player unchanged. |
| `RockPaperScissors` | Three rounds | Both agents answer concurrently from the same round state. |

Tic-Tac-Toe and Connect Four forfeit an agent after three consecutive failed attempts on its turn. Rock-Paper-Scissors treats an invalid choice as a forfeit because both choices are submitted simultaneously. Provider failures and malformed JSON are recorded in turn statistics instead of disappearing from the result.

For repeated matches, the original agent order is used for odd repetitions and reversed for even repetitions. Outcome summaries use a stable identity derived from provider, model, temperature, and seed, so wins stay attributed to the same configuration after seats change.

The built-in CLI uses the default game dimensions shown above. Library callers can construct custom game configurations and receive a structured error outcome when dimensions are invalid.

## Batch runs

Run the checked-in example:

```bash
cargo run --release --locked -- \
  --test-file examples/test_batch.csv \
  --output-jsonl batch-results.jsonl
```

CSV header names are case-insensitive. Optional columns may be omitted entirely or left blank. A present malformed value fails the row with its line number rather than silently falling back to a default.

| Column | Required | Default or constraint |
| --- | --- | --- |
| `game_name` | Yes | `TicTacToe`, `RockPaperScissors`, or `ConnectFour` |
| `agent_one_kind`, `agent_two_kind` | Yes | `OpenAI`, `Anthropic`, or `Ollama` |
| `agent_one_model`, `agent_two_model` | Yes | Non-empty provider model identifier |
| `agent_one_temp`, `agent_two_temp` | No | `0.7`; finite value from `0.0` through `2.0` |
| `agent_one_seed`, `agent_two_seed` | No | `0`; unsigned integer |
| `agent_one_secret_profile`, `agent_two_secret_profile` | No | Environment/default-profile resolution |
| `repetitions` | No | `1`; must be greater than zero |
| `description` | No | Empty text |
| `request_timeout_ms` | No | CLI value, normally `60000`; must be positive |
| `max_retries` | No | CLI value, normally `2` |
| `retry_backoff_ms` | No | CLI value, normally `250` |
| `max_total_tokens` | No | Unlimited; positive provider-reported token total |
| `max_concurrent_requests` | No | CLI value, normally `2`; must be positive |

See [`examples/test_batch.csv`](examples/test_batch.csv) for complete rows.

## Runtime safeguards

Provider calls use these match-level controls:

| CLI option | Default | Behavior |
| --- | ---: | --- |
| `--request-timeout-ms` | `60000` | Bounds each provider attempt, including response parsing. |
| `--max-retries` | `2` | Retries transient internal/provider errors and timeouts. |
| `--retry-backoff-ms` | `250` | Initial retry delay; each subsequent delay doubles. |
| `--max-concurrent-requests` | `2` | Caps requests in flight across both agents in a match. |
| `--max-total-tokens` | unlimited | Stops an agent after its cumulative reported usage reaches the limit. |

Invalid requests and invalid model responses are not retried. A timed-out provider might still finish and bill work remotely, so retries can duplicate cost even though the local future was cancelled.

Token budgets rely on provider-reported usage. OpenAI and Ollama expose usage when their APIs return it; a configured budget fails closed if usage is absent. Anthropic token budgets are rejected because the current connector does not expose usage.

## Results

Terminal output includes the authoritative outcome, total and average latency, invalid-move count, provider-reported token totals, every attempted move, and an aggregate outcome percentage for repeated or batch runs.

`--output-jsonl PATH` creates or truncates `PATH` and flushes one JSON object after every completed match. Each version 1 record includes:

- Schema, prompt-protocol, and arena versions.
- A UUID match ID and Unix-millisecond recording timestamp.
- Game name and repetition position.
- Agent seat, provider, model, temperature, requested/effective seed, and runtime policy.
- The explicit outcome and complete per-turn statistics, including state snapshots, validation errors, diagnostics, latency, and token usage.

Anthropic's effective seed is `null`; the current Anthropic API integration does not apply the requested seed. Keep `schema_version` and `prompt_protocol_version` when building analysis pipelines so future format or prompt changes can be compared intentionally.

## Provider capability matrix

| Capability | OpenAI | Anthropic | Ollama |
| --- | --- | --- | --- |
| Configured model | Yes | Yes | Yes |
| Temperature | Yes | Yes | Yes |
| Seed | Yes | No | Yes |
| Token usage | When reported | No | When reported |
| Named credential/endpoint profile | Yes | Yes | Yes |

Individual models can impose narrower temperature, seed, JSON-mode, or context constraints than the arena validates locally. Provider-side errors are preserved in the match outcome.

## Development

Run the same checks as CI:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --locked
```

The `ai_arena` library exposes agent contracts, provider configuration, games, batch parsing, reporting, and secrets resolution. Implement `GameAgent` to test games with another provider or a deterministic local double. Game engines are generic over that trait, so rule tests do not require live model APIs.

CI runs formatting, strict Clippy, and all tests for every pull request. The toolchain file keeps local and CI compiler behavior aligned.

## Known limitations

- Unit tests cover game rules and provider request configuration, but the repository does not run paid-provider integration tests in CI.
- Seat reversal reduces first-player bias; it does not make nondeterministic model outputs reproducible.
- Token limits are reactive because usage is known only after a response.
- Prompt protocol version 1 requests strict JSON through provider prompts/JSON mode rather than a shared tool-calling abstraction.
- Secrets are stored as plaintext TOML and should be protected with filesystem permissions.

## License

Licensed under the [Apache License 2.0](LICENSE).
