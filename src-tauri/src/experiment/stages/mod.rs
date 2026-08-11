//! Per-stage helpers for the experiment pipeline orchestrator.
//!
//! Each stage owns one FSM transition + the `runtime` call that drives
//! it. The orchestrator (`super::pipeline`) wires them together. Split
//! out from a 344-line monolith pipeline.rs (CLAUDE.md §1 — "≤ 300
//! lines for service modules"); the breakdown is one file per stage so
//! adding test fixtures targeted at a single stage stays cheap.

pub(super) mod analyze;
pub(super) mod generate;
pub(super) mod save;
pub(super) mod test_refine;

pub(super) use analyze::run_analyze_stage;
pub(super) use generate::{run_generate_stage, run_refine_path};
pub(super) use save::{emit_status, persist_artifacts, status};
