//! prefer-spy-on — prefer `vi.spyOn`/`jest.spyOn` over reassigning methods
//! with `vi.fn()`/`jest.fn()`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-spy-on",
    description: "Reassigning `obj.method = vi.fn()`/`jest.fn()` replaces the original implementation \
         and is harder to restore than a spy.",
    remediation: "Use vi.spyOn(obj, 'method') instead of reassigning to vi.fn()",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],

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
