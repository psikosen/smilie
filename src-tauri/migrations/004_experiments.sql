-- Smilie schema v4 — Experiments
-- See docs/plans/2026-04-26-experiments-design.md §3.
--
-- Per-paper reproducible Python experiments, with versioned forks for
-- prompt-driven refinement. Each row owns a {tests, code, inputs/outputs
-- schemas} tuple plus a status FSM:
--   analyzing → implausible
--             ↘ generating → testing → ready | failed

CREATE TABLE IF NOT EXISTS experiments (
  id                    TEXT PRIMARY KEY,
  paper_id              TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  parent_experiment_id  TEXT REFERENCES experiments(id) ON DELETE SET NULL,
  version               INTEGER NOT NULL DEFAULT 1,
  title                 TEXT NOT NULL,
  status                TEXT NOT NULL,
  plausibility_score    INTEGER,
  plausibility_reason   TEXT,
  approach_summary      TEXT,
  inputs_schema         TEXT NOT NULL DEFAULT '[]',
  outputs_schema        TEXT NOT NULL DEFAULT '[]',
  python_code           TEXT,
  python_tests          TEXT,
  test_attempts         INTEGER NOT NULL DEFAULT 0,
  test_last_output      TEXT,
  refine_prompt         TEXT,
  model_id              TEXT,
  created_at            INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_experiments_paper  ON experiments(paper_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_experiments_parent ON experiments(parent_experiment_id);

CREATE TABLE IF NOT EXISTS experiment_runs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  experiment_id   TEXT NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
  ts              INTEGER NOT NULL,
  inputs_json     TEXT NOT NULL,
  outputs_json    TEXT,
  error_message   TEXT,
  duration_ms     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_experiment_runs_exp_ts ON experiment_runs(experiment_id, ts DESC);
