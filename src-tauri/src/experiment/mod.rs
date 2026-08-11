//! Per-paper experiment pipeline.
//!
//! Separate from the per-paper learning-page pipeline (`pipeline::*`)
//! because the lifecycle is entirely different: experiments are
//! short-lived, user-triggered, and forkable. They DO reuse the
//! `ModelRuntime` trait + the same `AppHandle` for events.
//!
//! Status FSM:
//!   `analyzing → implausible | (generating → testing → ready | failed)`

mod pipeline;
mod stages;
mod test_loop;

pub use pipeline::{run_experiment_pipeline, ExperimentPipelineHandle};
