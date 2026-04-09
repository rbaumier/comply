//! rust-no-format-in-debug-impl — `format!` inside `Debug::fmt` allocates twice.
//!
//! `format!` builds a `String` by allocating, writing to it, then
//! returns it. If you then `write!(f, "{}", that_string)`, you've
//! done the work twice: once to build the temporary, once to copy
//! it into the formatter's writer. Just `write!(f, "...", args)`
//! directly — it streams into the writer with no intermediate.
//!
//! `Debug::fmt` is on the hot path for any structured logging
//! (every event with the type emitted), so even small inefficiencies
//! compound.

mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-format-in-debug-impl",
    description: "`format!` inside `Debug::fmt` allocates an extra `String` per call.",
    remediation: "Replace `format!(\"...\", x)` with a direct `write!(f, \"...\", x)` \
                  call. `write!` streams into the formatter's writer; \
                  `format!` builds an intermediate `String` that you \
                  immediately throw away.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::TreeSitter(Box::new(rust::Check)))],
    }
}
