//! no-default-params — reject `function f(x = 5)` style.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-default-params",
    description: "Default parameters hide behavior and create invisible coupling.",
    remediation: "Replace default parameters with explicit factory methods. \
                  `createUser(name, role = 'viewer')` → `createViewer(name)` \
                  and `createAdmin(name)`. Each factory is self-documenting \
                  and independently testable.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
            .collect(),
    }
}
