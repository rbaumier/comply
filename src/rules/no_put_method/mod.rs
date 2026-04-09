//! no-put-method — prefer PATCH over PUT for updates.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-put-method",
    description: "PUT replaces the entire resource; PATCH updates fields.",
    remediation: "Replace `method: 'PUT'` with `method: 'PATCH'` for \
                  partial updates. PUT requires you to send every field \
                  every time; PATCH accepts only the fields you want to \
                  change. Use PUT only when you genuinely want full \
                  replacement semantics, and comment why.",
    severity: Severity::Warning,
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
