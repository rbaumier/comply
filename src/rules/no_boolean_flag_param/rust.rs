//! rust: partially covered by `clippy::fn_params_excessive_bools`
//!
//! clippy ships `clippy::fn_params_excessive_bools` which flags functions
//! taking more than 3 bool parameters (configurable threshold). It's
//! weaker than comply's TypeScript rule — which flags EVERY boolean
//! parameter, not excessive counts — but it catches the worst cases.
//!
//! Enable in your crate root:
//!
//! ```ignore
//! #![warn(clippy::fn_params_excessive_bools)]
//! ```
//!
//! And lower the threshold in `clippy.toml`:
//!
//! ```toml
//! max-fn-params-bools = 0
//! ```
//!
//! Setting the threshold to 0 makes the lint equivalent to comply's strict
//! TypeScript rule — every bool parameter gets flagged, forcing the
//! "split into two functions" remediation.
//!
//! comply does not run clippy itself.
