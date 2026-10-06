//! Thin adapter over the [`rs_poker`] crate (Apache-2.0), which does the
//! actual 5/6/7-card evaluation via a perfect-hash lookup.
//!
//! This crate exists so the rest of the workspace depends on *our* small,
//! stable types rather than reaching into a third-party crate's types
//! directly — if we ever swap evaluators, only these files change.
//!
//! - [`evaluate_category`] / [`HandCategory`]: just the category (pair,
//!   flush, ...), no kickers.
//! - [`evaluate`] / [`EvaluatedHand`]: category *and* kickers, plus
//!   [`EvaluatedHand::describe`] for the tracker's strength label (spec
//!   section 11).
//! - [`strength`] / [`HandStrength`]: a fully comparable value (category
//!   *and* kickers folded into one `Ord`) for `offline equity helper` to compare
//!   hero vs. opponent hands with — a `HandCategory` alone can't tell two
//!   flushes apart by kicker, so equity needs this instead.

mod category;
mod evaluate;
mod strength;

pub use category::{evaluate_category, EvalError, HandCategory};
pub use evaluate::{evaluate, EvaluatedHand};
pub use strength::{strength, HandStrength};
