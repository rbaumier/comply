//! no-useless-error-capture-stack-trace — flag unnecessary Error.captureStackTrace() calls.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-error-capture-stack-trace",
    description: "Unnecessary `Error.captureStackTrace()` in Error subclass constructor.",
    remediation: "Remove the `Error.captureStackTrace(this, ClassName)` call. \
                  Built-in Error subclasses already capture the stack trace \
                  automatically via `super()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
