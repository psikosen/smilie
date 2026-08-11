//! Simplified SM-2 spaced repetition.
//!
//! Pure functions only — no I/O, no clock. The caller passes in the current
//! time and the prior `(ease, interval_days, reps)` tuple (or `None` for a
//! page that has never been quizzed) plus the most recent attempt outcome,
//! and we return the new state to persist.
//!
//! Rules:
//!   * First attempt seeds `ease = 2.5`, `interval = 1` day.
//!   * All-correct → `ease += 0.1` (cap 2.8); `interval = max(1, interval * ease)` days; `reps += 1`.
//!   * Any miss   → `ease -= 0.2` (floor 1.3); `interval = 1` day; `reps = 0`.
//!
//! Invariants:
//!   * `ease` stays in `[1.3, 2.8]`.
//!   * `interval_days` is `>= 1.0`.
//!   * `next_due_at_secs = now_secs + interval_days * 86_400` (rounded).
//!
//! See `tests` below for the locked behavior.

const SECS_PER_DAY: f64 = 86_400.0;
const EASE_FLOOR: f64 = 1.3;
const EASE_CAP: f64 = 2.8;
const SEED_EASE: f64 = 2.5;

#[derive(Debug, Clone, Copy)]
pub struct AttemptOutcome {
    pub correct: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct SrsUpdate {
    pub ease: f64,
    pub interval_days: f64,
    pub next_due_at_secs: i64,
    pub reps: i64,
}

/// Compute the next SRS state given prior state + the latest attempt.
///
/// `prior` is `Some((ease, interval_days, reps))` if the page has been
/// quizzed before, `None` for the first attempt.
pub fn next_state(
    prior: Option<(f64, f64, i64)>,
    now_secs: i64,
    outcome: AttemptOutcome,
) -> SrsUpdate {
    let all_correct = outcome.total > 0 && outcome.correct >= outcome.total;

    let (ease, interval_days, reps) = match prior {
        None => {
            // First attempt: seed and mark as 1 day out regardless of result.
            // A perfect first attempt still benefits from one consolidation pass.
            (SEED_EASE, 1.0, if all_correct { 1 } else { 0 })
        }
        Some((prev_ease, prev_interval, prev_reps)) => {
            if all_correct {
                let ease = (prev_ease + 0.1).min(EASE_CAP);
                let interval = (prev_interval * ease).max(1.0);
                (ease, interval, prev_reps + 1)
            } else {
                let ease = (prev_ease - 0.2).max(EASE_FLOOR);
                // Reset: missed at least one — back to a 1-day interval.
                let _ = prev_interval; // intentionally discarded
                (ease, 1.0, 0)
            }
        }
    };

    let next_due_at_secs = now_secs + (interval_days * SECS_PER_DAY).round() as i64;

    SrsUpdate {
        ease,
        interval_days,
        next_due_at_secs,
        reps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn first_attempt_seeds_defaults() {
        let out = next_state(
            None,
            1_000_000,
            AttemptOutcome {
                correct: 3,
                total: 3,
            },
        );
        assert!((out.ease - 2.5).abs() < 1e-9, "seed ease = 2.5");
        assert!((out.interval_days - 1.0).abs() < 1e-9, "seed interval = 1");
        assert_eq!(out.reps, 1, "perfect first attempt counts as 1 rep");
        assert_eq!(out.next_due_at_secs, 1_000_000 + DAY);
    }

    #[test]
    fn first_attempt_with_miss_does_not_advance_reps() {
        let out = next_state(
            None,
            0,
            AttemptOutcome {
                correct: 1,
                total: 3,
            },
        );
        assert_eq!(out.reps, 0);
        assert!((out.interval_days - 1.0).abs() < 1e-9);
    }

    #[test]
    fn all_correct_grows_interval_and_ease() {
        // After two days, we have ease 2.5, interval 1.
        let out = next_state(
            Some((2.5, 1.0, 1)),
            5 * DAY,
            AttemptOutcome {
                correct: 4,
                total: 4,
            },
        );
        assert!((out.ease - 2.6).abs() < 1e-9, "ease bumps by 0.1");
        // interval = 1.0 * 2.6 = 2.6 days
        assert!((out.interval_days - 2.6).abs() < 1e-9);
        assert_eq!(out.reps, 2);
        let expected_due = 5 * DAY + (2.6 * SECS_PER_DAY).round() as i64;
        assert_eq!(out.next_due_at_secs, expected_due);
    }

    #[test]
    fn miss_resets_interval_and_reps() {
        // 8 days into a successful run, the user misses one.
        let out = next_state(
            Some((2.7, 6.5, 4)),
            8 * DAY,
            AttemptOutcome {
                correct: 2,
                total: 4,
            },
        );
        assert!((out.ease - 2.5).abs() < 1e-9, "ease drops by 0.2");
        assert!((out.interval_days - 1.0).abs() < 1e-9);
        assert_eq!(out.reps, 0);
        assert_eq!(out.next_due_at_secs, 8 * DAY + DAY);
    }

    #[test]
    fn ease_floor_is_respected() {
        // Repeated failures should not push ease below 1.3.
        let mut state = (1.4, 5.0, 3);
        for _ in 0..10 {
            let out = next_state(
                Some(state),
                0,
                AttemptOutcome {
                    correct: 0,
                    total: 4,
                },
            );
            assert!(out.ease >= EASE_FLOOR - 1e-9);
            state = (out.ease, out.interval_days, out.reps);
        }
        assert!((state.0 - EASE_FLOOR).abs() < 1e-9);
    }

    #[test]
    fn ease_cap_is_respected() {
        let mut state = (2.7, 1.0, 1);
        for _ in 0..10 {
            let out = next_state(
                Some(state),
                0,
                AttemptOutcome {
                    correct: 4,
                    total: 4,
                },
            );
            assert!(out.ease <= EASE_CAP + 1e-9);
            state = (out.ease, out.interval_days, out.reps);
        }
        assert!((state.0 - EASE_CAP).abs() < 1e-9);
    }

    #[test]
    fn empty_quiz_treated_as_miss() {
        // A degenerate quiz with `total == 0` shouldn't promote the page.
        let out = next_state(
            Some((2.5, 4.0, 2)),
            0,
            AttemptOutcome {
                correct: 0,
                total: 0,
            },
        );
        assert!((out.interval_days - 1.0).abs() < 1e-9);
        assert_eq!(out.reps, 0);
    }
}
