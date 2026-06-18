mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-key-exists",
    description: "t() key is malformed (consecutive/leading/trailing dots, empty segments, or non-alphanumeric chars) and cannot resolve to a locale entry. Cross-file existence checks aren't performed.",
    remediation: "Fix the key shape so it matches `domain.subkey` with alphanumeric segments separated by single dots.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

fn is_malformed(inner: &str) -> bool {
    if inner.is_empty() {
        return false;
    }
    if inner.contains("..") || inner.ends_with('.') || inner.starts_with('.') {
        return true;
    }
    if inner.split('.').any(str::is_empty) {
        return true;
    }
    if inner
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'))
    {
        return true;
    }
    false
}

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
