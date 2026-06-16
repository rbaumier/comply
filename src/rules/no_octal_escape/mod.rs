//! no-octal-escape — disallow octal escape sequences in string literals.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-octal-escape",
    description: "Disallow octal escape sequences in string literals.",
    remediation: "Octal escape sequences are deprecated since ECMAScript 5. Replace the escape with \
                  a hexadecimal (`\\xA9`) or unicode (`\\u00A9`) escape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["suspicious"],

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
