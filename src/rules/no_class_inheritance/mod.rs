//! no-class-inheritance

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-class-inheritance",
    description: "Class inheritance (`extends`) creates tight coupling — prefer composition over inheritance.",
    remediation: "Use composition, mixins, or dependency injection instead of class inheritance.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["functional"],
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
