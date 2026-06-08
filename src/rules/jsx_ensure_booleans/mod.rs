//! jsx-ensure-booleans — require the left-hand side of `{x && <Y />}` to be a
//! boolean expression.
//!
//! React renders `0`, `""`, and `NaN` as text, so `{count && <List />}` with
//! `count === 0` visibly renders "0". This rule enforces that conditional JSX
//! rendering uses an explicit boolean: comparisons, `!x`, `!!x`, a boolean
//! literal, or an identifier whose name reads as a boolean
//! (`is…`, `has…`, `should…`, …).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "jsx-ensure-booleans",
    description: "Left-hand side of `{x && <Jsx />}` must be an unambiguous boolean.",
    remediation: "Coerce with `!!x`, compare (`x > 0`, `x != null`), or use a ternary `cond ? <Jsx /> : null`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
