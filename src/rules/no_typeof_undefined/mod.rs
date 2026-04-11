//! no-typeof-undefined — flag `typeof x === 'undefined'`.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-typeof-undefined",
    description: "Compare with `undefined` directly instead of using `typeof`.",
    remediation: "Replace `typeof x === 'undefined'` with `x === undefined`. \
                  Modern JS engines handle `undefined` safely; the `typeof` \
                  guard is no longer necessary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
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
