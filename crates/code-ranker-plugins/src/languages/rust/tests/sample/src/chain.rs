//! Module `chain` — a deliberate **3-node `uses` cycle** (one → two → three →
//! one) so the golden covers the `chain` cycle kind (a 3+-member SCC), alongside
//! the 2-member `mutual` cycle that `a ⇄ b` already pins. Only `uses` edges are
//! flow, so this trio is a genuine cycle; classified `chain` because its SCC has
//! three members.
pub mod one;
pub mod two;
pub mod three;
