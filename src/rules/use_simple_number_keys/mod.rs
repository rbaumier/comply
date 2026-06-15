//! use-simple-number-keys — disallow non-base-10 / underscore-separated
//! numeric object member names.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "use-simple-number-keys",
    description: "Disallow number literal object member names which are not base 10 or use an underscore separator.",
    remediation: "Write the object member name as a plain base-10 number (e.g. `16` instead of `0x10`, `1000` instead of `1_000`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["complexity"],

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
