//! rust-ptr-arg — `&str`/`&[T]`/`&Path` over `&String`/`&Vec<T>`/`&PathBuf`.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-ptr-arg",
    description: "Prefer borrowed slices over borrowed owned types.",
    remediation: "Replace `&String` with `&str`, `&Vec<T>` with `&[T]`, \
                  `&PathBuf` with `&Path`. The slice form accepts more \
                  caller types with no extra cost. Enable `clippy::ptr_arg`.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
