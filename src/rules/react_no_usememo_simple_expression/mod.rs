//! react-no-usememo-simple-expression — `useMemo` wrapping a trivially cheap
//! expression where the memo overhead exceeds the computation cost.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-usememo-simple-expression",
    description: "`useMemo` wrapping a trivially cheap expression — memo overhead exceeds the computation.",
    remediation: "Remove the `useMemo` wrapper and compute the value inline. \
                  Memoizing primitives, simple property access, or basic arithmetic \
                  costs more than the computation itself.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
