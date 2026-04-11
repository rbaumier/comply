//! prefer-export-from — use `export { x } from` for re-exports.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "prefer-export-from",
    description: "Prefer `export { x } from './m'` over import-then-re-export.",
    remediation: "Replace `import { x } from './m'; export { x };` with \
                  `export { x } from './m';`. Direct re-export is shorter, \
                  avoids a binding in the local scope, and makes the re-export \
                  intent explicit.",
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
