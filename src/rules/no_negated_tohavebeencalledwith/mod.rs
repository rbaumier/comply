//! no-negated-tohavebeencalledwith — flag `expect(x).not.toHaveBeenCalledWith(...)`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-negated-tohavebeencalledwith",
    description: "Disallow negated `toHaveBeenCalledWith` assertions, which pass whenever the mock was called with any other arguments and so never fail.",
    remediation: "Use `expect(fn).not.toHaveBeenCalled()` or assert over `fn.mock.calls`",
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
