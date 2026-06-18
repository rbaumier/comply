//! prefer-set-size — flag `[...mySet].length` -> `mySet.size`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prefer-set-size",
    description: "Prefer `Set#size` instead of spreading into an array and reading `.length`.",
    remediation: "Replace `[...mySet].length` or `Array.from(mySet).length` \
                  with `mySet.size`. Spreading a Set into an array just to \
                  read its length is wasteful — `Set#size` is O(1).",
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
