//! no-process-global — discourage use of the Node `process` global.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-process-global",
    description: "Usage of the Node `process` global is discouraged — it is hard for tools to \
                  statically analyze.",
    remediation: "Import `process` explicitly with `import process from \"node:process\";` instead \
                  of relying on the implicit global.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/no-process-global/"),
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
