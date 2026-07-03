//! `chain::one` → `chain::two` (the first link of the 3-node cycle).
use crate::chain::two::two;

pub fn one() -> i32 {
    two()
}
