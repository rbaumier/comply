mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-top-level-literals",
    description: "JSON top-level value is a bare literal.",
    remediation: "Wrap the value in an array or object — some JSON parsers only accept an object or array as the root element.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["suspicious"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
