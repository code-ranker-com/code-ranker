//! `chain::two` → `chain::three` (second link).
use crate::chain::three::three;

pub fn two() -> i32 {
    three()
}
