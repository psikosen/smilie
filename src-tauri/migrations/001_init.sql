-- Smilie schema v1
-- Corresponds to docs/plans/2026-04-24-smilie-design.md §5.1

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS papers (
  id                TEXT PRIMARY KEY,
  title             TEXT NOT NULL,
  authors           TEXT NOT NULL DEFAULT '[]',
  file_path         TEXT NOT NULL,
  original_filename TEXT,
  page_count        INTEGER NOT NULL DEFAULT 0,
  thumbnail_path    TEXT,
  processing_status TEXT NOT NULL DEFAULT 'queued',
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_papers_status ON papers(processing_status);

CREATE TABLE IF NOT EXISTS pages (
  id                 TEXT PRIMARY KEY,
  paper_id           TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  title              TEXT NOT NULL,
  type               TEXT NOT NULL,
  source_sections    TEXT NOT NULL DEFAULT '[]',
  source_pages       TEXT NOT NULL DEFAULT '[]',
  html_path          TEXT NOT NULL DEFAULT '',
  source_json        TEXT,
  model_id           TEXT,
  prompt_hash        TEXT,
  validation_status  TEXT,
  status             TEXT NOT NULL DEFAULT 'queued',
  order_index        INTEGER NOT NULL DEFAULT 0,
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pages_paper_order ON pages(paper_id, order_index);
CREATE INDEX IF NOT EXISTS idx_pages_status ON pages(status);

CREATE TABLE IF NOT EXISTS ocr_blocks (
  id           TEXT PRIMARY KEY,
  paper_id     TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  page_number  INTEGER NOT NULL,
  block_type   TEXT NOT NULL,
  text         TEXT,
  bbox         TEXT,
  confidence   REAL,
  source_model TEXT
);

CREATE INDEX IF NOT EXISTS idx_ocr_paper_page ON ocr_blocks(paper_id, page_number);

CREATE TABLE IF NOT EXISTS jobs (
  id             TEXT PRIMARY KEY,
  paper_id       TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  stage          TEXT NOT NULL,
  status         TEXT NOT NULL,
  progress       REAL NOT NULL DEFAULT 0,
  current_model  TEXT,
  error_message  TEXT,
  retry_count    INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,
  updated_at     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
CREATE INDEX IF NOT EXISTS idx_jobs_paper ON jobs(paper_id);

CREATE TABLE IF NOT EXISTS assets (
  id       TEXT PRIMARY KEY,
  paper_id TEXT REFERENCES papers(id) ON DELETE CASCADE,
  kind     TEXT NOT NULL,
  path     TEXT NOT NULL,
  meta     TEXT
);

CREATE TABLE IF NOT EXISTS figures (
  id          TEXT PRIMARY KEY,
  paper_id    TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  page_number INTEGER NOT NULL,
  bbox        TEXT,
  caption     TEXT,
  image_path  TEXT
);

CREATE TABLE IF NOT EXISTS tables_ (
  id          TEXT PRIMARY KEY,
  paper_id    TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  page_number INTEGER NOT NULL,
  bbox        TEXT,
  caption     TEXT,
  data_json   TEXT
);

CREATE TABLE IF NOT EXISTS equations (
  id          TEXT PRIMARY KEY,
  paper_id    TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
  page_number INTEGER NOT NULL,
  latex       TEXT,
  context     TEXT
);

CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS generation_logs (
  id      INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id  TEXT NOT NULL,
  ts      INTEGER NOT NULL,
  level   TEXT NOT NULL,
  message TEXT NOT NULL
);

-- FTS5 virtual table for page search
CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
  title,
  body,
  page_id UNINDEXED,
  paper_id UNINDEXED
);
