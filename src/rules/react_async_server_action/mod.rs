//! react-async-server-action — server actions must be async.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "react-async-server-action",
    description: "Server actions (functions with `\"use server\"`) must be `async`.",
    remediation: "Add `async` to the function. React Server Actions must be async \
                  functions — a synchronous function with `\"use server\"` will \
                  cause a build error or runtime failure.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
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
