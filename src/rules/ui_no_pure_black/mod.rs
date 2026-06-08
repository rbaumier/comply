//! ui-no-pure-black — flag pure black (`#000`, `#000000`, `rgb(0,0,0)`, `black`).

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-pure-black",
    description: "Pure black (`#000`, `rgb(0,0,0)`, `black`) looks harsh on screens — prefer a near-black.",
    remediation: "Use a slightly warmer/softer tone such as `#0a0a0a` or an OKLCH near-black.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
