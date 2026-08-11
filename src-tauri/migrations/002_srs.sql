PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS quiz_attempts (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  page_id         TEXT    NOT NULL,
  ts              INTEGER NOT NULL,
  correct_count   INTEGER NOT NULL,
  total_count     INTEGER NOT NULL,
  FOREIGN KEY (page_id) REFERENCES pages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_quiz_attempts_page_ts
  ON quiz_attempts(page_id, ts DESC);

CREATE TABLE IF NOT EXISTS srs_state (
  page_id         TEXT    PRIMARY KEY,
  ease_factor     REAL    NOT NULL DEFAULT 2.5,
  interval_days   REAL    NOT NULL DEFAULT 1.0,
  next_due_at     INTEGER NOT NULL,
  reps            INTEGER NOT NULL DEFAULT 0,
  updated_at      INTEGER NOT NULL,
  FOREIGN KEY (page_id) REFERENCES pages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_srs_state_due ON srs_state(next_due_at);
