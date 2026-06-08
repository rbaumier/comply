//! prisma-no-findunique-without-where — `findUnique` without `where` returns null.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "prisma-no-findunique-without-where",
    description: "`findUnique` without a `where` argument always resolves to null.",
    remediation: "Pass `{ where: { id } }`, or switch to `findFirst` if filtering on a non-unique field.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["prisma"],

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
