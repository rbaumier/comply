//! no-unassigned-import

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-unassigned-import",
    description: "Side-effect import with no specifiers — assign the import or remove it.",
    remediation: "Import specific bindings (`import { x } from '…'`) or remove the import if the side-effect is unnecessary. CSS/style imports are allowed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports"],
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
