//! no-submit-handler-without-preventDefault — require `onSubmit` JSX handlers
//! to call `event.preventDefault()`.
//!
//! Without `preventDefault`, the browser performs a full-page navigation on
//! submit, wiping any client-side state. Controlled React forms almost always
//! want this call. The rule is scoped to inline arrow / function expressions
//! so it does not false-positive on handlers defined elsewhere.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-submit-handler-without-preventDefault",
    description: "Inline `onSubmit={...}` handler does not call `preventDefault()`.",
    remediation: "Call `event.preventDefault()` at the top of the handler, or switch to a form action that doesn't navigate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
