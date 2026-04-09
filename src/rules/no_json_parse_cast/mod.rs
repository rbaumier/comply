//! no-json-parse-cast — validate JSON.parse output, don't cast.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-json-parse-cast",
    description: "`JSON.parse(x) as T` is a lie — validate the runtime shape.",
    remediation: "Replace the cast with runtime validation: \
                  `const parsed = UserSchema.safeParse(JSON.parse(raw))` \
                  (Zod) or a hand-written type guard that inspects the value.",
    severity: Severity::Error,
    doc_url: None,
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
