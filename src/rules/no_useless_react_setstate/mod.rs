//! no-useless-react-setstate

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-useless-react-setstate",
    description: "Calling a `useState` setter with its own state value is a no-op.",
    remediation: "Remove the useless `setState` call or pass a different value. `setX(x)` triggers a re-render but does not change state.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
