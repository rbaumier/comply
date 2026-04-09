//! no-misleading-collection-name — flag `*List` named as `Set`/`Map`/etc.
//!
//! From the coding-standards skill: "misleading names — `userList` but it's
//! a Set → `userSet`". A name that lies about the underlying type forces
//! every reader to double-check the declaration. Worse, callers reach for
//! the wrong API (`.length` on a Set, `[i]` on a Map).

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-misleading-collection-name",
    description: "Variable name lies about the underlying collection type.",
    remediation: "Rename the binding to match the actual type — `userList` holding \
                  a `Set` becomes `userSet`, `nameMap` holding an `Array` becomes \
                  `nameList`. The name and the type must agree.",
    severity: Severity::Error,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
        ],
    }
}
