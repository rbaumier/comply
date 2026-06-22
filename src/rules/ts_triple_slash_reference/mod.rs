//! ts-triple-slash-reference — disallow `/// <reference ... />` directives.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-triple-slash-reference",
    description: "Triple-slash `path` references to source modules are legacy — use ES `import`.",
    remediation: "Replace `/// <reference path=\"./mod.ts\" />` with an ES `import` declaration. (`types`/`lib` references and `path` references to `.d.ts` declaration files have no ESM equivalent and are not flagged.)",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/triple-slash-reference"),
    categories: &["typescript"],

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
