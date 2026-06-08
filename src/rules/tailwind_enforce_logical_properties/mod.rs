//! tailwind-enforce-logical-properties — `ml-*` / `pl-*` / etc. → logical equivalents.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-enforce-logical-properties",
    description: "Physical directional spacing (`ml-`, `mr-`, `pl-`, `pr-`) doesn't flip in RTL — prefer logical (`ms-`, `me-`, `ps-`, `pe-`).",
    remediation: "Replace `ml-4` with `ms-4`, `pr-2` with `pe-2`, etc. The logical pair flips automatically when the writing direction is RTL.",
    severity: Severity::Warning,
    doc_url: Some("https://tailwindcss.com/docs/margin#using-logical-properties"),
    categories: &["tailwind", "internationalization"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// True if any class in `s` uses a physical directional spacing prefix
/// that has a logical equivalent.
pub(crate) fn has_physical_directional_spacing(s: &str) -> Option<&'static str> {
    const PAIRS: &[(&str, &str)] = &[
        ("ml-", "ms-"),
        ("mr-", "me-"),
        ("pl-", "ps-"),
        ("pr-", "pe-"),
        ("-ml-", "-ms-"),
        ("-mr-", "-me-"),
        ("-pl-", "-ps-"),
        ("-pr-", "-pe-"),
    ];
    for token in s.split_whitespace() {
        // Strip variant prefixes like `hover:`, `md:`, `[&:nth-child(2)]:`.
        let after_variants = token.rsplit(':').next().unwrap_or(token);
        for (phys, logical) in PAIRS {
            if let Some(rest) = after_variants.strip_prefix(phys)
                && !rest.is_empty()
            {
                return Some(logical);
            }
        }
    }
    None
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
mod logical_tests {
    use super::*;

    #[test]
    fn detects_physical_classes() {
        assert_eq!(has_physical_directional_spacing("ml-4"), Some("ms-"));
        assert_eq!(has_physical_directional_spacing("pr-2"), Some("pe-"));
        assert_eq!(has_physical_directional_spacing("hover:pl-3"), Some("ps-"));
        assert_eq!(has_physical_directional_spacing("-ml-2"), Some("-ms-"));
    }

    #[test]
    fn ignores_logical_classes() {
        assert!(has_physical_directional_spacing("ms-4 me-2 ps-1 pe-1").is_none());
    }

    #[test]
    fn ignores_other_classes() {
        assert!(has_physical_directional_spacing("m-4 p-2 mt-1 mb-2").is_none());
    }
}
