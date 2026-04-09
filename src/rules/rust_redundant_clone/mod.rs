//! rust-redundant-clone — don't clone values that could be moved or borrowed.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-redundant-clone",
    description: "Remove `.clone()` calls whose result isn't independently observed.",
    remediation: "Move the value instead of cloning it, or borrow it if the \
                  caller still needs access. Clones allocate and copy — \
                  they're never free. Enable `clippy::redundant_clone`.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![],
    }
}
