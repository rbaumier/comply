//! error-without-cause — flag `new Error(e.message)` without `{ cause: e }`.
//!
//! When wrapping a caught error in a new one, the original stack trace and
//! `.cause` chain MUST be preserved. `new Error(e.message)` strips both —
//! callers debugging at 2am will see the wrapped message but lose the
//! original location, type, and any nested cause. The fix is the second
//! argument: `new Error("...", { cause: e })`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
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
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
        ],
    }
}
