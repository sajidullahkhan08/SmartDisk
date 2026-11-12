//! Offline, explainable classification and deletion-risk assessment.
//!
//! The classifier identifies an item's likely purpose from path, extension,
//! attributes and folder context. The risk engine separately evaluates how
//! risky removal would be. Categories never decide deletion safety.

pub mod classifier;
pub mod features;
pub mod risk;

pub use classifier::{classify_detailed, classify_entry, Classification};
pub use features::PathContext;
pub use risk::{assess, assess_detailed, is_protected_path, RiskReport};
