mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "regex-documented-with-semantics",
    description: "Complex regex (>20 chars) without a comment explaining its purpose.",
    remediation: "Add a comment above the regex explaining what it matches.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

const MIN_PATTERN_LEN: usize = 20;

/// True if `pattern` consists of a plain literal optionally bracketed
/// by `^` and `$` anchors — no metacharacters, no quantifiers, no
/// groups, no escapes. Such a regex IS its own documentation; a
/// comment restating the literal adds nothing.
pub(crate) fn is_simple_anchored_literal(pattern: &str) -> bool {
    let inner = pattern
        .strip_prefix('^')
        .unwrap_or(pattern)
        .strip_suffix('$')
        .unwrap_or_else(|| pattern.strip_prefix('^').unwrap_or(pattern));
    if inner.is_empty() {
        return false;
    }
    // Any metacharacter or escape disqualifies — including `^`/`$`
    // that appear in the middle, which we treat as anchors-out-of-place.
    !inner
        .chars()
        .any(|c| matches!(c, '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']'
            | '|' | '\\' | '{' | '}' | '^' | '$'))
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn accepts_plain_anchored_sentence() {
        assert!(is_simple_anchored_literal(
            "^Type invalide : chaîne attendu, nombre reçu$"
        ));
    }

    #[test]
    fn accepts_partial_anchor() {
        assert!(is_simple_anchored_literal("^starts here"));
        assert!(is_simple_anchored_literal("ends here$"));
    }

    #[test]
    fn rejects_pattern_with_metacharacters() {
        assert!(!is_simple_anchored_literal("^\\d+ users$"));
        assert!(!is_simple_anchored_literal("^(foo|bar)$"));
        assert!(!is_simple_anchored_literal("^abc.*xyz$"));
    }

    #[test]
    fn rejects_empty_pattern() {
        assert!(!is_simple_anchored_literal(""));
        assert!(!is_simple_anchored_literal("^$"));
    }
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
