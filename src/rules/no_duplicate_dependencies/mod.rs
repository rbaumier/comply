mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-dependencies",
    description: "A dependency is listed twice in the same `package.json` section, or in two \
                  sections that should be mutually exclusive — both confuse package managers \
                  about which version wins.",
    remediation: "Remove one of the listings so each dependency appears in exactly one section.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["suspicious", "package-json"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
