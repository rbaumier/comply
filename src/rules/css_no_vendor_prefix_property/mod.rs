mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "css-no-vendor-prefix-property",
    description: "Vendor-prefixed CSS properties are unnecessary with modern autoprefixer tooling.",
    remediation: "Remove the vendor prefix (e.g. `-webkit-transform` → `transform`) and rely on autoprefixer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["css"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
