//! operation-returning-nan

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "operation-returning-nan",
    description: "Arithmetic operation will produce `NaN`.",
    remediation: "Convert the operand to a number first (`Number(x)`, `parseInt(x)`, `+x`) or fix the expression. Arithmetic on `undefined` or non-numeric strings always returns `NaN`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["code-quality"],
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
