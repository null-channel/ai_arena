# DuckDB analytics

AI Arena keeps each match in `runs` and every resolved turn in `run_turns`. The application leaderboard is derived from the same durable history; these queries are useful for ad hoc analysis.

```sql
-- Completion and reliability by game
SELECT
  game_type,
  count(*) AS runs,
  avg(total_turns) AS average_turns,
  avg(invalid_moves) AS average_invalid_moves,
  avg(total_duration_ms) AS average_duration_ms
FROM runs
WHERE status = 'completed'
GROUP BY game_type
ORDER BY runs DESC;
```

```sql
-- Slowest players across completed games
SELECT
  player,
  count(*) AS turns,
  avg(time_taken_ms) AS average_turn_ms,
  sum(CASE WHEN valid THEN 0 ELSE 1 END) AS invalid_moves
FROM run_turns
GROUP BY player
ORDER BY average_turn_ms DESC;
```

```sql
-- Matrix-job results. `matrix_job_id` is null for ordinary CLI and web runs.
SELECT
  matrix_job_id,
  status,
  count(*) AS runs
FROM runs
WHERE matrix_job_id IS NOT NULL
GROUP BY matrix_job_id, status
ORDER BY matrix_job_id, status;
```
