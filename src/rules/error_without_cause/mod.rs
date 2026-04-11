//! error-without-cause — flag `new Error(e.message)` without `{ cause: e }`.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "error-without-cause",
    description: "new Error(e.message) drops the original stack — pass { cause: e }.",
    remediation: "When wrapping a caught error, preserve the original stack and chain: \
                  `throw new Error('high-level message', { cause: original })`. \
                  Without `cause`, the debugger sees the wrapped message but loses \
                  the source location, type, and nested cause chain.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
