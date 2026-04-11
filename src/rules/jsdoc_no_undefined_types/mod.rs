//! jsdoc-no-undefined-types

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsdoc-no-undefined-types",
    description: "JSDoc `@param`/`@returns` type is not a known built-in.",
    remediation: "Fix the type name in the JSDoc tag. Common built-ins: string, number, boolean, Array, Object, Promise, Function, Date, RegExp, Map, Set, Symbol, Error, void, null, undefined, any, never.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["jsdoc"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
