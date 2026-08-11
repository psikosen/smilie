/**
 * Typed wrappers over Tauri `invoke` and event listeners.
 * Single source of truth for the Rust <-> Svelte command surface.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn, type EventCallback } from "@tauri-apps/api/event";
import { isTauri } from "./tauri-guard";

export class TauriUnavailableError extends Error {
  readonly command: string;

  constructor(command: string) {
    super(`Tauri command "${command}" is unavailable outside the desktop app`);
    this.name = "TauriUnavailableError";
    this.command = command;
  }
}

export class IpcCommandError extends Error {
  readonly command: string;
  override readonly cause: unknown;

  constructor(command: string, cause: unknown) {
    const detail =
      cause instanceof Error
        ? cause.message
        : typeof cause === "string"
          ? cause
          : stringifyUnknown(cause);
    super(`${command} failed: ${detail}`);
    this.name = "IpcCommandError";
    this.command = command;
    this.cause = cause;
  }
}

function stringifyUnknown(value: unknown): string {
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (typeof record.message === "string") return record.message;
    if (typeof record.error === "string") return record.error;
  }
  try {
    const json = JSON.stringify(value);
    return json ?? String(value);
  } catch {
    return String(value);
  }
}

async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) throw new TauriUnavailableError(command);
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new IpcCommandError(command, error);
  }
}

async function listenEvent<T>(
  event: string,
  callback: EventCallback<T>,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  try {
    return await listen<T>(event, callback);
  } catch (error) {
    throw new IpcCommandError(`listen:${event}`, error);
  }
}

export type PaperId = string;
export type PageId = string;
export type JobId = string;

export type PageType =
  | "informational"
  | "concept"
  | "formula"
  | "figure"
  | "table"
  | "quiz"
  | "simulation";

export type ProcessingStatus = "queued" | "running" | "failed" | "complete";

export interface PaperSummary {
  id: PaperId;
  title: string;
  authors: string[];
  page_count: number;
  processing_status: ProcessingStatus;
  created_at: number;
  updated_at: number;
  thumbnail_path: string | null;
}

export type ReadingLevel = "eli5" | "default" | "expert";

export interface PageRecord {
  id: PageId;
  paper_id: PaperId;
  title: string;
  type: PageType;
  source_sections: string[];
  source_pages: number[];
  html_path: string;
  status: "queued" | "generating" | "ready" | "failed";
  validation_status: "valid" | "failed" | "pending" | null;
  order_index: number;
  /** The currently-displayed level for this page. The level cache is
   *  managed server-side; switching levels via `setPageReadingLevel`
   *  is instant if cached, kicks regen otherwise. */
  current_level: ReadingLevel;
  /** Levels that have been generated + cached server-side. */
  cached_levels: ReadingLevel[];
  created_at: number;
  updated_at: number;
}

export interface PaperDetail {
  paper: PaperSummary;
  pages: PageRecord[];
  active_job: JobSummary | null;
}

export type PipelineStage =
  | "import_pdf"
  | "render_pages"
  | "ocr_extract"
  | "structure_paper"
  | "build_learning_plan"
  | "generate_pages"
  | "validate_pages"
  | "save_pages"
  | "index_pages"
  | "complete";

export interface JobSummary {
  id: JobId;
  paper_id: PaperId;
  stage: PipelineStage;
  status: "pending" | "running" | "paused" | "failed" | "completed";
  progress: number;
  current_model: string | null;
  error_message: string | null;
  retry_count: number;
}

export type ModelRole = "ocr" | "reasoning" | "generator";

export type RuntimeStatus =
  | { kind: "ready"; runtime: string; model: string }
  | { kind: "missing_weights"; runtime: string; expected_path: string }
  | { kind: "error"; message: string };

export interface RuntimeStatusReport {
  ocr: RuntimeStatus;
  reasoning: RuntimeStatus;
  generator: RuntimeStatus;
}

export interface Settings {
  ocr_model_path: string;
  reasoning_model_path: string;
  generator_model_path: string;
  runtime_backend: string;
  context_length: number;
  temperature: number;
  max_tokens: number;
  thinking_mode: boolean;
  interactive_aggressiveness: number;
  offline_only: boolean;
}

export type SettingsPatch = Partial<Settings>;

export interface DuePage {
  page_id: PageId;
  paper_id: PaperId;
  title: string;
  due_at: number;
}

// ─── Experiments ────────────────────────────────────────────────
export type ExperimentId = string;

export type ExperimentStatus =
  | "analyzing"
  | "generating"
  | "testing"
  | "ready"
  | "failed"
  | "implausible";

export type InputSpec =
  | { name: string; kind: "number"; default: number; min?: number; max?: number; step?: number; label: string; help?: string }
  | { name: string; kind: "int";    default: number; min?: number; max?: number; step?: number; label: string; help?: string }
  | { name: string; kind: "choice"; default: string; options: string[]; label: string; help?: string }
  | { name: string; kind: "text";   default: string; label: string; help?: string }
  | { name: string; kind: "vector"; default: number[]; length: number; label: string; help?: string };

export type OutputSpec =
  | { name: string; kind: "number"; label: string; precision?: number }
  | { name: string; kind: "text";   label: string }
  | { name: string; kind: "table";  label: string; columns: string[] }
  | { name: string; kind: "series"; label: string; x_label?: string; y_label?: string };

export interface ExperimentSummary {
  id: ExperimentId;
  paper_id: PaperId;
  parent_experiment_id: ExperimentId | null;
  version: number;
  title: string;
  status: ExperimentStatus;
  plausibility_score: number | null;
  refine_prompt: string | null;
  created_at: number;
  updated_at: number;
}

export interface ExperimentDetail extends ExperimentSummary {
  plausibility_reason: string | null;
  approach_summary: string | null;
  inputs_schema: InputSpec[];
  outputs_schema: OutputSpec[];
  python_code: string | null;
  python_tests: string | null;
  test_attempts: number;
  test_last_output: string | null;
  model_id: string | null;
}

export interface ExperimentRun {
  id: number;
  experiment_id: ExperimentId;
  ts: number;
  inputs: Record<string, unknown>;
  outputs: Record<string, unknown> | null;
  error_message: string | null;
  duration_ms: number | null;
}

export type ExperimentEventPayload = {
  type: "experiment_status";
  experiment_id: ExperimentId;
  status: ExperimentStatus;
  score?: number;
  error?: string;
};

// ─── Comparisons (two-paper diff) ───────────────────────────────
export type ComparisonId = string;
export type ComparisonStatus = "analyzing" | "ready" | "failed";

export interface ComparisonSummary {
  id: ComparisonId;
  paper_a_id: PaperId;
  paper_b_id: PaperId;
  /** Denormalized titles so the picker / list can render without a
   *  follow-up paper lookup. */
  paper_a_title: string;
  paper_b_title: string;
  status: ComparisonStatus;
  created_at: number;
  updated_at: number;
}

export interface ComparisonDetail extends ComparisonSummary {
  /** Path to the rendered diff page on disk. Same structured-block
   *  HTML shape as a learning page — the existing iframe viewer can
   *  load it without any new render code. Null until status=ready. */
  html_path: string | null;
  approach_summary: string | null;
  error_message: string | null;
}

export type ComparisonEventPayload = {
  type: "comparison_status";
  comparison_id: ComparisonId;
  status: ComparisonStatus;
  error?: string;
};

// ─── Commands ─────────────────────────────────────────────────

export const ipc = {
  ingestPdf: (filePath: string) =>
    invokeCommand<PaperId>("ingest_pdf", { filePath }),
  listPapers: () => invokeCommand<PaperSummary[]>("list_papers"),
  getPaper: (id: PaperId) => invokeCommand<PaperDetail>("get_paper", { id }),
  getPage: (id: PageId) => invokeCommand<PageRecord>("get_page", { id }),
  regeneratePage: (id: PageId) => invokeCommand<void>("regenerate_page", { id }),
  exportPage: (id: PageId, dest: string) =>
    invokeCommand<string>("export_page", { id, dest }),
  deletePaper: (id: PaperId) => invokeCommand<void>("delete_paper", { id }),
  pauseJob: (id: JobId) => invokeCommand<void>("pause_job", { id }),
  resumeJob: (id: JobId) => invokeCommand<void>("resume_job", { id }),
  cancelJob: (id: JobId) => invokeCommand<void>("cancel_job", { id }),
  getSettings: () => invokeCommand<Settings>("get_settings"),
  updateSettings: (patch: SettingsPatch) =>
    invokeCommand<void>("update_settings", { patch }),
  listRuntimeStatus: () =>
    invokeCommand<RuntimeStatusReport>("list_runtime_status"),
  recheckRuntimeStatus: () =>
    invokeCommand<RuntimeStatusReport>("recheck_runtime_status"),
  revealModelsFolder: () => invokeCommand<string>("reveal_models_folder"),
  searchPages: (query: string) =>
    invokeCommand<PageRecord[]>("search_pages", { query }),
  recordQuizAttempt: (pageId: PageId, correct: number, total: number) =>
    invokeCommand<void>("record_quiz_attempt", { pageId, correct, total }),
  listDuePages: () => invokeCommand<DuePage[]>("list_due_pages"),
  // ── Experiments ──
  analyzeExperiment: (paperId: PaperId) =>
    invokeCommand<ExperimentSummary>("analyze_experiment", { paperId }),
  listExperimentsForPaper: (paperId: PaperId) =>
    invokeCommand<ExperimentSummary[]>("list_experiments_for_paper", { paperId }),
  getExperiment: (id: ExperimentId) =>
    invokeCommand<ExperimentDetail>("get_experiment", { id }),
  refineExperiment: (parentId: ExperimentId, prompt: string) =>
    invokeCommand<ExperimentSummary>("refine_experiment", { parentId, prompt }),
  forceGenerateExperiment: (id: ExperimentId) =>
    invokeCommand<ExperimentSummary>("force_generate_experiment", { id }),
  deleteExperiment: (id: ExperimentId) =>
    invokeCommand<void>("delete_experiment", { id }),
  recordExperimentRun: (
    experimentId: ExperimentId,
    inputs: Record<string, unknown>,
    outputs: Record<string, unknown> | null,
    errorMessage: string | null,
    durationMs: number | null,
  ) =>
    invokeCommand<void>("record_experiment_run", {
      experimentId,
      inputs,
      outputs,
      errorMessage,
      durationMs,
    }),
  listExperimentRuns: (experimentId: ExperimentId) =>
    invokeCommand<ExperimentRun[]>("list_experiment_runs", { experimentId }),
  // ── PDF tab (open original) ──
  getPaperPdfUrl: (paperId: PaperId) =>
    invokeCommand<string>("get_paper_pdf_url", { paperId }),
  openPaperInDefaultApp: (paperId: PaperId) =>
    invokeCommand<void>("open_paper_in_default_app", { paperId }),
  // ── Reading-level slider ──
  setPageReadingLevel: (pageId: PageId, level: ReadingLevel) =>
    invokeCommand<void>("set_page_reading_level", { pageId, level }),
  // ── Two-paper diff ──
  compareWithPaper: (paperAId: PaperId, paperBId: PaperId) =>
    invokeCommand<ComparisonSummary>("compare_with_paper", { paperAId, paperBId }),
  listComparisonsForPaper: (paperId: PaperId) =>
    invokeCommand<ComparisonSummary[]>("list_comparisons_for_paper", { paperId }),
  getComparison: (id: ComparisonId) =>
    invokeCommand<ComparisonDetail>("get_comparison", { id }),
  deleteComparison: (id: ComparisonId) =>
    invokeCommand<void>("delete_comparison", { id }),
};

// ─── Events ────────────────────────────────────────────────────

export type PipelineEventPayload =
  | { type: "stage_started"; job_id: JobId; paper_id: PaperId; stage: PipelineStage }
  | { type: "stage_progress"; job_id: JobId; stage: PipelineStage; progress: number }
  | { type: "stage_completed"; job_id: JobId; stage: PipelineStage }
  | { type: "stage_failed"; job_id: JobId; stage: PipelineStage; error: string }
  | { type: "page_ready"; page_id: PageId; paper_id: PaperId };

export const events = {
  onPipeline: (cb: EventCallback<PipelineEventPayload>): Promise<UnlistenFn> =>
    listenEvent<PipelineEventPayload>("pipeline", cb),
  onPaperUpdated: (cb: EventCallback<PaperId>): Promise<UnlistenFn> =>
    listenEvent<PaperId>("paper:updated", cb),
  onRuntimeStatusChanged: (
    cb: EventCallback<RuntimeStatusReport>,
  ): Promise<UnlistenFn> =>
    listenEvent<RuntimeStatusReport>("runtime:status_changed", cb),
  onExperiment: (
    cb: EventCallback<ExperimentEventPayload>,
  ): Promise<UnlistenFn> =>
    listenEvent<ExperimentEventPayload>("experiment", cb),
  onComparison: (
    cb: EventCallback<ComparisonEventPayload>,
  ): Promise<UnlistenFn> =>
    listenEvent<ComparisonEventPayload>("comparison", cb),
};
