//! no-function-declaration-in-block

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-function-declaration-in-block",
    description: "Function declaration inside a control-flow block has inconsistent hoisting behavior.",
    remediation: "Move the function declaration to the top level, or use a `const fn = () => { ... }` expression instead. Function declarations in blocks are only conditionally hoisted in sloppy mode and forbidden in strict mode by some engines.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
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
