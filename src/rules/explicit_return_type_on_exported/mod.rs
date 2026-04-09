//! explicit-return-type-on-exported — exported functions need explicit return types.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "explicit-return-type-on-exported",
    description: "Exported functions must declare their return type.",
    remediation: "Add an explicit `: ReturnType` annotation after the \
                  parameters of every exported function. This locks the \
                  public contract and prevents silent drift when the \
                  implementation changes.",
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
