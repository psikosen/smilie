-- Smilie schema v3 — FTS cascade triggers
--
-- `pages_fts` is a contentless FTS5 virtual table, so SQLite cannot
-- attach a real foreign-key cascade to it. Without a trigger the FTS
-- rows are orphaned every time a `pages` row is removed (paper delete,
-- `clear_pages_for_paper` re-runs, etc.). Catching the deletion at
-- the trigger level means every present and future delete path is
-- covered without callers having to remember.

CREATE TRIGGER IF NOT EXISTS pages_fts_after_delete
AFTER DELETE ON pages
BEGIN
  DELETE FROM pages_fts WHERE page_id = OLD.id;
END;
