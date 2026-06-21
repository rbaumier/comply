//! no-process-global — discourage use of the Node `process` global.
//!
//! The concern is runtime portability: production code that relies on the
//! implicit `process` global breaks in browser/edge/Deno runtimes. Test files
//! (`skip_in_test_dir`) always run in Node, where `process` is a legitimate
//! global — spying/mocking it (`vi.spyOn(process, "exit")`, reassigning
//! `process.cwd`) and reading `process.env` for test setup are standard Node
//! idioms, so the portability concern does not apply and they are not flagged.

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

    skip_in_test_dir: true,
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
