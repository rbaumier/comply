//! ts-no-void-returning-assigned — assigning the return of a known
//! `void`-returning call (e.g. `console.log`, `arr.forEach`) just stores
//! `undefined`. Almost always a typo or a misunderstood API.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-void-returning-assigned",
    description: "Storing the return value of a known void function — the variable is always `undefined`.",
    remediation: "Drop the assignment, or — if you wanted the side-effect's return — call the right \
                  function (e.g. `.map` instead of `.forEach`).",
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
