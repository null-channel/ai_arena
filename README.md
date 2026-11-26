# ai_arena
A place for AI's to test their metal against each other in 'the arena'

## Concept
An arena where different AI agents can compete against each other in various games or challenges.

**Current Games:**
- Tic-Tac-Toe
- Rock-Paper-Scissors
- Connect Four
- Chess
- Checkers
- Battleship
- TODO: Maze (who can escape the fastest/fewest turns)

## Features
- Modular design to easily add new games and AI agents. The initial engine supports turn-based games.
- Support for OpenAI, Anthropic, Gemini, and Ollama (API and self-hosted).
- **CLI** subcommands: `run`, `batch`, `serve`, `secrets`
- **HTMX web UI**: served directly by the Rust server at `/` with no npm build step
- CSV batch runner with per-row agent configuration
- Turn-by-turn statistics and formatted tables
- DuckDB run history for CLI, CSV, and web-launched games

## Usage

### Subcommands

```bash
ai_arena run --help      # single game from CLI
ai_arena batch --help    # CSV batch
ai_arena replay --help   # replay a persisted match
ai_arena serve --help    # HTTP + WebSocket server
ai_arena secrets --help  # manage ~/.config/ai_arena/secrets.toml
```

### Running a Single Game

```bash
ai_arena run \
  --game-name TicTacToe \
  --agent-one-kind OpenAI \
  --agent-one-model gpt-4o-mini \
  --agent-one-temp 0.7 \
  --agent-one-seed 42 \
  --agent-two-kind Ollama \
  --agent-two-model llama3 \
  --agent-two-temp 0.7 \
  --agent-two-seed 43 \
  --repetitions 1 \
  --db-path ai_arena.duckdb
```

### Running Batch Games (CSV)

```bash
ai_arena batch --test-file examples/test_batch.csv --db-path ai_arena.duckdb
```

Agent kinds in CSV: `OpenAI`, `Anthropic`, `Gemini`, `Ollama`.

For a larger local run, `examples/demo_bulk_run.csv` schedules 195 matches across every game. It uses `llama3` through Ollama at `http://localhost:11434`, so start Ollama and make that model available first. The batch runner records every match in DuckDB and executes CSV rows sequentially.

```bash
ai_arena batch --test-file examples/demo_bulk_run.csv --db-path bulk-demo.duckdb
```

Reduce the `repetitions` values in that file for a quick smoke test, or replace the two agent columns with any supported provider configuration.

### Replaying a Match

```bash
ai_arena replay <run-id> --db-path ai_arena.duckdb
```

The CLI prints each stored turn and its resulting board state. Completed runs also have a replay link in the web session list.

### Matrix Runs

Multi repeats one matchup. Matrix schedules every unordered pair of roster entries for the chosen number of matches per pair. Three agents at three matches per pair produce nine matches. The earlier roster entry plays in the first seat. Each batch supports up to 1,000 total matches and configurable concurrency. Each simulation receives its own run ID, persists its turns in DuckDB, and links to its live observer or replay page. The JSON API accepts the same configuration:

```json
POST /api/matrix
{
  "game_type": "TicTacToe",
  "agents": [/* two or more agent configurations */],
  "repetitions": 100,
  "concurrency_limit": 4
}
```

Poll `GET /api/matrix/{matrix_id}` for queued, running, completed, and failed counts.

### Web Server

Start the API and HTMX UI from one Rust process:

```bash
ai_arena serve --host 127.0.0.1 --port 3000 --db-path ai_arena.duckdb
```

When binding beyond localhost, set `--auth-token` (or `AI_ARENA_AUTH_TOKEN`) and send `Authorization: Bearer <token>` to every route except `/api/health`.

The HTMX form is seeded from the same agent flags used by `run`, so you can start the server with your preferred defaults:

```bash
ai_arena serve \
  --game-name ConnectFour \
  --agent-one-kind open-ai \
  --agent-one-model gpt-5.4 \
  --agent-two-kind ollama \
  --agent-two-model qwen3.6:latest \
  --agent-two-api-endpoint http://localhost:11434
```

Form changes are cached in memory while the server is running and reused as defaults for the next game.

Open `http://127.0.0.1:3000/` to choose Single, Multi, or Matrix. All three modes accept custom model names. The provider and model determine which settings appear. For example, Ollama exposes its endpoint and temperature, while Anthropic uses the model’s sampling defaults. Axum serves the UI and its assets directly.

### Saving results

The **Save results** checkbox in the navigation menu starts unchecked on every server launch. Leave it unchecked for **Test mode** (green), or check it to save new results (red). The setting is shared across browser tabs. Each game, batch, Tool Calling benchmark, and Turing Test round keeps the setting it started with.

Test games and benchmark reports stay in memory. Test rounds reveal their votes on screen. Only saved runs contribute to saved history, game rankings, and viewer standings. Tool Calling uses the runner’s `--no-save` option in Test mode. Select **Offline mock** separately when you want to avoid model calls.

### Tool Calling

Open **Tool Calling** at `/toolbench`. Use **Add model** to build a comparison. Choose **Custom model** to add a model once or save it to the picker for future runs. **Edit model IDs** accepts a pasted list of `provider:model` names. Select **Offline mock** to exercise the runner without provider calls. An empty family filter selects the whole dataset. Key profiles use toolbench's named keys (for example `openai=work,anthropic=personal`), with its usual provider environment variables as the fallback. Unattended runs require credentials that toolbench can read without prompting.

Each launch runs the sibling `toolbench run` with uploads disabled. Open a run to see progress, per-model accuracy and failure counts, and the expected and selected tools for each case. Benchmark history survives server restarts.

| Server environment variable | Default |
| --- | --- |
| `ARENA_TOOLBENCH_MODELS` | User config directory under `ai_arena/toolbench-models.toml` (on Linux, `~/.config/ai_arena/toolbench-models.toml` by default) |
| `ARENA_TOOLBENCH_BIN` | Sibling `../tool_call_tests/target/{release,debug}/toolbench`, then `toolbench` on PATH |
| `ARENA_TOOLBENCH_DATA` | Sibling `../tool-call-test-data`, containing `tools/` and `test_cases/` |
| `ARENA_TOOLBENCH_RESULTS` | User-local data directory under `ai-arena/toolbench/` (on Linux, `~/.local/share/ai-arena/toolbench/` by default) |

The picker starts with Arena presets. **Save & add** writes the full list to the local TOML file. You can also edit its `models` array directly. Presets are model IDs for the runner’s providers. Availability depends on provider access or which Ollama models are installed.

Build the runner in its own checkout with `cargo build --bin toolbench` if needed. Keep the dataset external. Each run gets its own external directory for its report, call log, runner diagnostics and local bundle. Arena's game database stores game results only.

### Twitch Turing Test

Start the shared Twitch Interactions service from the Arena checkout with `../twitch-interactions/quickstart.sh`. On the machine running that service, put `TC_USERNAME` and `TC_PASSWORD` (a Twitch OAuth access token) in `~/.twitch_chat`. You can also set `TC_CHANNEL` there. The service reads the file at startup and accepts plain or `export` assignments. Environment variables override the corresponding file values.

In Arena, open **Turing Test → Round setup → Twitch connection**. Enter the service URL as seen from the Arena server. **Test connection** checks the channel and event feed. **Save URL** stores the address in the Arena configuration directory under `ai_arena/turing-chat.toml` for future rounds. A round in progress keeps its original connection.

Choose a sample and set review and voting times. Put **Audience screen** (`/turing/screen`) on stream or in OBS. When the countdown changes from **Review** to **Vote**, viewers can type `!vote human` or `!vote ai`. The last valid choice from each Twitch user ID within the voting window counts. At reveal, the screen shows the source, vote totals, and chat’s verdict.

**Viewer standings** (`/turing/scores`) ranks accuracy across completed rounds, with highest-first and lowest-first views. Ties use participation then login. Viewers who skip a round receive no score for it. Completed results survive restarts. Cancelled rounds, interrupted rounds and rounds with missing chat history do not enter the standings.

| Server environment variable | Default |
| --- | --- |
| `ARENA_TWITCH_URL` | `http://127.0.0.1:8787` (used until a service URL is saved in Arena) |
| `ARENA_TURING_CONTENT` | The sibling summer event library, `../../2026/summer/events/turing-test/content` relative to the Arena checkout |
| `ARENA_TURING_RESULTS` | The local user data directory under `ai-arena/turing/`, one JSON file per scored round |

To add a sample, create a folder under `text/`, `art_visual/`, `voice/`, or `music/` in the content library. Put the media and a `metadata.json` in that folder. For example:

```json
{"id":"postcard-01","title":"Postcard","true_source":"Human","display_file":"postcard.png"}
```

Use `AI` or `Human` for the source and a neutral title for viewers. Supported files include plain text and Markdown (displayed as text), PNG, JPEG, WebP, GIF, SVG and audio. A `transcript_file` can stand in for a missing display file. The event library includes rehearsal material. Select samples with known provenance for a scored stream.

### DuckDB Run History

CLI runs and web games launched with **Save results** checked are stored in DuckDB. By default the database file is `./ai_arena.duckdb`, relative to the current working directory. Use `--db-path` on `run`, `batch`, or `serve` to choose a different file.

The web UI keeps active sessions in memory for live polling, then reads completed and failed run history back from DuckDB so previous runs remain visible after a server restart.

**HTMX routes:**
- `GET /` — single-match setup and live matches
- `GET /multi` and `GET /matrix` — setup and saved batches for each mode
- `GET /multi/{id}` and `GET /matrix/{id}` — batch progress and links to every session
- `GET /history?game=Chess` — per-game outcomes, turn counts, invalid moves, durations, and links to saved matches
- `GET /rankings?game=Chess` — model leaderboard for the selected game (defaults to Tic-tac-toe). Expand a model to see its record against each opponent.
- `POST /games/start` — create and launch a game from a form
- `POST /games/defaults` — cache form defaults in memory
- `GET /games` — session list fragment
- `GET /games/{id}` — game observation page
- `GET /games/{id}/fragment` — game status/result fragment
- `GET /games/{id}/replay` — step through a persisted match

**API routes:**
- `GET /api/health` — liveness
- `GET /api/games` — list active sessions and persisted completed/error runs
- `POST /api/games` — create session (JSON: `game_type`, `agents`)
- `GET /api/games/{id}` — session status
- `GET /api/runs/{id}` — persisted run details
- `GET /api/runs/{id}/events` — ordered persisted turns
- `POST /api/matrix` — start a bounded-parallel matrix job
- `GET /api/matrix/{id}` — matrix progress and child run IDs
- `GET /api/leaderboard?game=Chess` — model wins, losses, draws, turn-quality aggregates, and head-to-head records for one game. Omit `game` for combined rankings.
- `GET /api/secrets` — list secret profile names
- `WS /ws/games/{id}` — watch a pending game run

### Secrets

Profiles live in `~/.config/ai_arena/secrets.toml` (see `plans/secret-configuration-separation.md`).

```bash
ai_arena secrets get openai.default.api_key
ai_arena secrets set my.key value
ai_arena secrets encrypt
```

Use `--agent-one-secret-profile` / `--agent-two-secret-profile` on `run`, or `secret_profile` in CSV / web POST body.

### CSV File Format

| Column | Required | Description |
|--------|----------|-------------|
| `game_name` | Yes | `TicTacToe`, `RockPaperScissors`, `ConnectFour`, `Chess`, `Checkers`, `Battleship` |
| `agent_one_kind` | Yes | `OpenAI`, `Anthropic`, `Gemini`, `Ollama` |
| `agent_one_model` | Yes | Model name |
| `agent_one_temp` | No | Default 0.7 |
| `agent_one_seed` | No | Default 0 |
| `agent_one_secret_profile` | No | Profile from secrets file |
| `agent_one_api_endpoint` | No | Ollama base URL |
| `agent_two_*` | Yes/No | Same as agent one |
| `repetitions` | No | Default 1 |
| `description` | No | Free text |

Example:

```csv
game_name,agent_one_kind,agent_one_model,agent_one_temp,agent_one_seed,agent_two_kind,agent_two_model,agent_two_temp,agent_two_seed,repetitions,description
TicTacToe,OpenAI,gpt-4o-mini,0.7,42,Gemini,gemini-2.0-flash,0.7,43,1,OpenAI vs Gemini
```

### Output Format

When running games from the CLI, you get a summary, turn-by-turn table, and per-player stats (see existing table output in `src/games/display.rs`).

## Environment Variables

If no secrets file is configured, API keys can fall back to environment variables:

```bash
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
export GEMINI_API_KEY="your-gemini-key"
export OLLAMA_BASE_URL="http://localhost:11434"
```

## Custom Prompt Files

Use `--agent-one-prompt-file` or `--agent-two-prompt-file` on `run`/`serve`, or the matching `agent_one_prompt_file` / `agent_two_prompt_file` CSV columns. The file contents are appended to that agent’s system guidance at runtime. Only the path is saved in run history, so keep credentials out of prompt files.

The web form also accepts a prompt-file path for each agent. The control room includes a DuckDB-backed leaderboard that aggregates completed run history without exposing secret values or prompt contents.

## Roadmap

See `plans/ai-arena-roadmap.md` and `TODOS.md` for HTMX UI, persistence/replay, game matrix, and parallel runs.

## Open Questions
- Do we want an "allow cheating" mode where AIs can break rules or call each other out?
