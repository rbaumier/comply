//! jsx-no-leaked-render

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "jsx-no-leaked-render",
    description: "Numeric value used with `&&` in JSX renders `0` instead of nothing.",
    remediation: "Convert to boolean: `{!!count && <Component />}` or use a ternary: `{count > 0 ? <Component /> : null}`.",
    severity: Severity::Error,
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
