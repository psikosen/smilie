-- Smilie schema v5
--   1. Two-paper comparisons (cross-paper diff).
--   2. Per-page reading-level cache so the eli5/default/expert toggle is
--      instant once a level has been generated once.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS comparisons (
  id                  TEXT PRIMARY KEY,
  paper_a_id          TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  paper_b_id          TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  status              TEXT NOT NULL,
  html_path           TEXT,
  approach_summary    TEXT,
  error_message       TEXT,
  model_id            TEXT,
  created_at          INTEGER NOT NULL,
  updated_at          INTEGER NOT NULL,
  UNIQUE(paper_a_id, paper_b_id)
);

CREATE INDEX IF NOT EXISTS idx_comparisons_a ON comparisons(paper_a_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_comparisons_b ON comparisons(paper_b_id, created_at DESC);

-- Reading-level support on the existing pages table. SQLite ADD COLUMN
-- with DEFAULT is safe on existing rows (the engine fills the default in
-- without a table rewrite).
ALTER TABLE pages ADD COLUMN current_level TEXT NOT NULL DEFAULT 'default';
-- level_cache: JSON object {<level>: <html_path>} populated as each level
-- is generated. The "default" entry is written by `generate_pages` on
-- the very first generation pass so switching back to "default" is
-- always instant.
ALTER TABLE pages ADD COLUMN level_cache TEXT NOT NULL DEFAULT '{}';
