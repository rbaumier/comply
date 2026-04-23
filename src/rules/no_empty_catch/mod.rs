//! no-empty-catch — flag empty catch blocks that silently swallow errors.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-empty-catch",
    description: "Disallow empty catch blocks that silently swallow errors.",
    remediation: "Handle the error explicitly — log it, rethrow, or add a comment \
                  explaining why swallowing is intentional.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
