//! ts-no-object-keys-typed-loop — `Object.keys(obj).forEach(k => obj[k])` is
//! type-unsound: TypeScript types `Object.keys` as `string[]`, not
//! `(keyof Obj)[]`, so `obj[k]` ends up `any`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-object-keys-typed-loop",
    description: "`Object.keys(obj).forEach(k => obj[k])` is type-unsound — TS widens to `any`.",
    remediation: "Use `for (const [k, v] of Object.entries(obj))`, or cast: \
                  `(Object.keys(obj) as Array<keyof typeof obj>).forEach(...)`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
