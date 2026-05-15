//! tailwind-no-arbitrary-value — flag any `[…]` arbitrary value.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-arbitrary-value",
    description: "Arbitrary values `[…]` bypass design system tokens — each one is a small drift away from the design.",
    remediation: "Replace the arbitrary value with the matching design token. Add a custom token in `tailwind.config.ts` if the value is genuinely needed in multiple places.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

/// True if any class in `s` contains a `[…]` arbitrary value, excluding
/// known structural uses (`&:hover`-style variants, `aria-[...]`,
/// `data-[...]`, `group-[...]`) where `[]` is part of the variant
/// selector syntax, not an arbitrary value.
pub(crate) fn has_arbitrary_value(s: &str) -> bool {
    for token in s.split_whitespace() {
        // Strip variant prefixes (`hover:`, `md:`, …) — they may contain
        // `[]` themselves (`[&:nth-child(2)]:`), but a `[` AFTER the last
        // variant separator `:` is an arbitrary VALUE.
        let last_colon = token.rfind(':');
        let value_part = match last_colon {
            Some(idx) => &token[idx + 1..],
            None => token,
        };
        if value_part.contains('[') && value_part.contains(']') {
            return true;
        }
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
mod arbitrary_tests {
    use super::*;

    #[test]
    fn detects_arbitrary_value() {
        assert!(has_arbitrary_value("p-[16px]"));
        assert!(has_arbitrary_value("text-[#fff]"));
        assert!(has_arbitrary_value("md:p-[4px]"));
    }

    #[test]
    fn ignores_variant_only_brackets() {
        assert!(!has_arbitrary_value("[&:nth-child(2)]:p-4"));
        assert!(!has_arbitrary_value("aria-[expanded=true]:bg-red-500"));
    }

    #[test]
    fn ignores_design_token_classes() {
        assert!(!has_arbitrary_value("p-4 m-2 text-red-500"));
    }
}
