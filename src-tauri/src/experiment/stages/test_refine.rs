//! Stage 3 — test → refine loop (entry point).
//!
//! The loop's logic lives in `super::super::test_loop` because the
//! `python3` harness + child-management code is sizable enough to
//! warrant its own module. This file exists so the `stages/` tree
//! mirrors the per-stage layout used in `pipeline/stages/` for the
//! per-paper pipeline. Add fixtures targeted at the loop here when
//! expanding test coverage.

// `run_test_and_refine_loop` is consumed via its canonical path
// `crate::experiment::test_loop::run_test_and_refine_loop`. Re-exporting
// here would just duplicate the symbol and trip the unused-import
// lint, so this file stays a marker comment for the stages/ layout.
#[allow(dead_code)]
pub(crate) const _STAGE_DOC: &str = "test → refine loop entry point lives in experiment::test_loop";
