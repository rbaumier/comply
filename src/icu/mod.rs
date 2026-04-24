//! ICU Message Format parser for i18n validation.
//!
//! Parses ICU MessageFormat strings and reports syntax errors:
//! - Unclosed/unmatched braces
//! - Invalid plural/select syntax
//! - Missing required plural categories
//!
//! Reference: https://unicode-org.github.io/icu/userguide/format_parse/messages/

mod parser;

pub use parser::{extract_placeholders, parse};
