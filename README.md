# ai_arena
A place for AI's to test their metal against each other in 'the arena'

## Concept
An arena where different AI agents can compete against each other in various games or challenges. 

**Current Games:**
- ✅ Tic-Tac-Toe
- ✅ Rock-Paper-Scissors
- ✅ Connect Four

**Planned Games:**
- Chess
- Checkers

## Features
- Modular design to easily add new games and AI agents. The initial engine supports "turn based" games.
- Support for many different AI Models including self-hosted and API-based models. Current support: OpenAI, Anthropic, Ollama.
- Two ways to run games:
  - **Command Line**: Run individual games with detailed statistics
  - **CSV Batch**: Run multiple game configurations from a CSV file
- Comprehensive turn-by-turn statistics tracking for analysis
- Beautiful formatted output using tables

## Usage

### Running a Single Game (Command Line)

Run a single game with detailed statistics output:

```bash
ai_arena \
  --game-name TicTacToe \
  --agent-one-kind OpenAI \
  --agent-one-model gpt-4o-mini \
  --agent-one-temp 0.7 \
  --agent-one-seed 42 \
  --agent-two-kind Ollama \
  --agent-two-model llama3 \
  --agent-two-temp 0.7 \
  --agent-two-seed 43 \
  --repetitions 1
```

### Running Batch Games (CSV File)

Run multiple game configurations from a CSV file:

```bash
ai_arena --test-file examples/test_batch.csv
```

### CSV File Format

The CSV file should have the following columns:

| Column | Required | Description | Example Values |
|--------|----------|-------------|----------------|
| `game_name` | ✅ Yes | Name of the game | `TicTacToe`, `RockPaperScissors`, `ConnectFour` |
| `agent_one_kind` | ✅ Yes | Type of first agent | `OpenAI`, `Anthropic`, `Ollama` |
| `agent_one_model` | ✅ Yes | Model name for first agent | `gpt-4o-mini`, `llama3`, `claude-3-7-sonnet` |
| `agent_one_temp` | ❌ No | Temperature for first agent (default: 0.7) | `0.0` to `1.0` |
| `agent_one_seed` | ❌ No | Random seed for first agent (default: 0) | Any integer |
| `agent_two_kind` | ✅ Yes | Type of second agent | `OpenAI`, `Anthropic`, `Ollama` |
| `agent_two_model` | ✅ Yes | Model name for second agent | `gpt-4o-mini`, `llama3`, `claude-3-7-sonnet` |
| `agent_two_temp` | ❌ No | Temperature for second agent (default: 0.7) | `0.0` to `1.0` |
| `agent_two_seed` | ❌ No | Random seed for second agent (default: 0) | Any integer |
| `repetitions` | ❌ No | Number of times to run this game (default: 1) | Any positive integer |
| `description` | ❌ No | Optional description for this test case | Any string |

#### Example CSV File

```csv
game_name,agent_one_kind,agent_one_model,agent_one_temp,agent_one_seed,agent_two_kind,agent_two_model,agent_two_temp,agent_two_seed,repetitions,description
TicTacToe,OpenAI,gpt-4o-mini,0.7,42,OpenAI,gpt-4o-mini,0.7,43,1,OpenAI vs OpenAI TicTacToe
RockPaperScissors,Ollama,llama3,0.7,100,Ollama,llama3,0.8,101,3,Best of 3 Rock Paper Scissors
ConnectFour,Anthropic,claude-3-7-sonnet,0.7,200,OpenAI,gpt-4o-mini,0.7,201,1,Connect Four Championship
TicTacToe,Ollama,llama3,0.5,300,OpenAI,gpt-4o-mini,0.9,301,2,TicTacToe with different temperatures
```

**Visual Representation:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CSV Batch File Structure                                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Row 1: Headers (column names)                                             │
│  Row 2+: Test cases (one per row)                                         │
│                                                                             │
│  Each row defines:                                                         │
│  • Which game to play                                                      │
│  • Two AI agents to compete                                                │
│  • Their configurations (model, temperature, seed)                        │
│  • How many times to repeat                                                │
│  • Optional description                                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Output Format

When running games, you'll see formatted statistics including:

1. **Game Summary**
   - Winner or draw status
   - Total duration
   - Number of turns
   - Average turn time
   - Invalid moves count

2. **Turn-by-Turn Table**
   - Each move with player, move details, timing, and validity

3. **Player Statistics**
   - Aggregated stats per player (turns, valid/invalid moves, timing)

Example output:
```
================================================================================
GAME RESULTS: TicTacToe
================================================================================

📊 GAME SUMMARY
--------------------------------------------------------------------------------
🏆 Winner: OpenAI_1 (X)
⏱️  Total Duration: 2.34s
🔄 Total Turns: 9
⚡ Average Turn Time: 260.00ms
❌ Invalid Moves: 0

📋 TURN-BY-TURN STATISTICS
--------------------------------------------------------------------------------
┌──────┬─────────────┬──────────────┬───────────┬───────┬───────┐
│ Turn │ Player      │ Move         │ Time (ms) │ Valid │ Error │
├──────┼─────────────┼──────────────┼───────────┼───────┼───────┤
│ 1    │ OpenAI_1    │ row: 1, col: │ 245       │ ✓     │ -     │
│      │             │ 1            │           │       │       │
...
```

## Environment Variables

Make sure to set the required API keys:

```bash
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
export OLLAMA_BASE_URL="http://localhost:11434"  # Optional, defaults to localhost
export OLLAMA_MODEL="llama3"  # Optional, defaults to llama3
```

## Open Questions
- Do we want to have a "allow cheating" mode where AI's are given the ability to cheat? What would this look like? would it be optional and up to the AI if they cheat or not? would it give them the ability to make moves that are not allowed by the rules? could the other AI call out the cheating AI?
