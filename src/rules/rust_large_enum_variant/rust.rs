//! rust: covered by `clippy::large_enum_variant`
//!
//! Enable at the crate root:
//!
//! ```ignore
//! #![warn(clippy::large_enum_variant)]
//! ```
//!
//! Enum size equals the largest variant. A single big variant bloats
//! every instance of the enum even when the small variant is the
//! common case. Box the large variant to keep the enum compact.
//!
//! comply does not run clippy itself.
