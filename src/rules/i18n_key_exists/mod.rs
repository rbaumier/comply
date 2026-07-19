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
    severity: Severity::Error,
    doc_url: None,
    categories: &["i18n"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

/// A key is well-formed when it is a chain of non-empty segments separated by
/// single dots, each segment built from the allowed character class:
/// ASCII alphanumerics plus `.` `-` `_` `$`. `$` is a conventional i18n
/// namespace-prefix character in Vue/JS systems (`$vuetify.*`, `$t()`), so a
/// `$`-prefixed key is treated as well-formed. Keys carrying any other
/// character (e.g. `{`/`}`/backtick from an interpolated template) are
/// malformed and cannot resolve to a static locale entry.
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
        .any(|c| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '$'))
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

#[cfg(test)]
mod tests {
    use super::oxc_typescript::Check;
    use crate::rules::test_helpers::run_rule;

    fn flags(src: &str) -> bool {
        !run_rule(&Check, src, "t.ts").is_empty()
    }

    #[test]
    fn accepts_dollar_prefixed_framework_namespace_keys() {
        // Vuetify's locale adapter provides `$vuetify.*` keys: well-formed, not flagged.
        assert!(!flags("t('$vuetify.monthPicker.range.title')"));
        assert!(!flags("t('$vuetify.monthPicker.header')"));
        assert!(!flags("t('$vuetify.timePicker.am')"));
        assert!(!flags("t('$vuetify.datePicker.title')"));
    }

    #[test]
    fn accepts_ordinary_dotted_keys() {
        assert!(!flags("t('foo.bar.baz')"));
        assert!(!flags("t('home_page.title-line')"));
    }

    #[test]
    fn flags_interpolated_keys_with_forbidden_chars() {
        // `{`/`}` stay disallowed even now that `$` is permitted.
        assert!(flags("t('foo.${bar}')"));
        assert!(flags("t('foo.{count}.label')"));
    }

    #[test]
    fn flags_malformed_dot_shapes() {
        assert!(flags("t('foo..bar')"));
        assert!(flags("t('.leading')"));
        assert!(flags("t('trailing.')"));
    }
}
