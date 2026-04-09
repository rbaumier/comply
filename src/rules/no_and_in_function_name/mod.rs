//! no-and-in-function-name — flag function names like `getUserAndUpdateCache`.
//!
//! `And` in a function name is a CQS (Command-Query Separation) violation:
//! the function does TWO things, so callers can't compose either one in
//! isolation. The fix is to split into two functions and let the caller
//! call them in sequence — `getUser()` then `updateCache(user)`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-and-in-function-name",
    description: "`And` in a function name signals two responsibilities — split it.",
    remediation: "A function with `And` in its name does two things. Split into \
                  two functions named after each responsibility, then let the caller \
                  compose them: `getUserAndUpdateCache` → `getUser()` + `updateCache(user)`.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
        ],
    }
}
