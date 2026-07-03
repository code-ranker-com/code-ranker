//! `chain::three` → `chain::one` — closes the three-node `uses` cycle.
use crate::chain::one::one;

pub fn three() -> i32 {
    one()
}
