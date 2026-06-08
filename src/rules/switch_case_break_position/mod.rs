//! switch-case-break-position — flag `break`/`return`/`continue`/`throw`
//! placed outside the block in a `case` clause.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "switch-case-break-position",
    description: "`break`/`return` should be inside the case block, not after it.",
    remediation: "Move the `break`/`return`/`continue`/`throw` statement \
                  inside the `{ }` block of the case clause. Placing it \
                  outside creates an inconsistent style where the block looks \
                  complete but the terminator dangles after the closing brace.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],

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
