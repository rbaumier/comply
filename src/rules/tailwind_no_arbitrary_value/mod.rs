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

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// CSS function markers — exempt computed/dynamic values from the arbitrary-value check.
const CSS_FN_MARKERS: &[&str] = &[
    "var(",
    "calc(",
    "oklch(",
    "clamp(",
    "min(",
    "max(",
    "radial-gradient(",
    "linear-gradient(",
    "conic-gradient(",
    "--theme(",
];

/// Canonical CSS units that have semantic meaning (typographic scale,
/// container queries, responsive viewport) and have no equivalent Tailwind token scale.
/// `%` is included because percentage values beyond what Tailwind's fraction utilities
/// cover (e.g. `200%` for pseudo-element height overlays) have no token equivalent.
const CANONICAL_UNIT_SUFFIXES: &[&str] = &[
    "%",
    "ch", "cap", "lh", "cqi", "cqb", "cqw", "cqh", "cqmin", "cqmax",
    "svh", "svw", "dvh", "dvw", "lvh", "lvw", "rlh",
];

/// Returns true if `value` (the text between `[` and `]`) is a numeric
/// quantity expressed in a canonical CSS unit with no token equivalent.
fn is_canonical_unit_value(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    for suffix in CANONICAL_UNIT_SUFFIXES {
        if let Some(numeric) = digits.strip_suffix(suffix) {
            if numeric.parse::<f64>().is_ok() {
                return true;
            }
        }
    }
    false
}

/// True if any class in `s` contains a `[…]` arbitrary value, excluding:
/// - Arbitrary VARIANT selectors (`word-[[…]]:utility` — double `[[` in the value part).
/// - Values that use CSS custom properties (`var(--…)`).
/// - Values that use CSS functions (`calc(`, `oklch(`, `clamp(`, etc.).
/// - Numeric values in canonical CSS units (`ch`, `cap`, `lh`, `cqi`, …).
pub(crate) fn has_arbitrary_value(s: &str) -> bool {
    for token in s.split_whitespace() {
        // Strip variant prefixes (`hover:`, `md:`, …) — a `[` AFTER the last
        // variant separator `:` is an arbitrary VALUE.
        let last_colon = token.rfind(':');
        let value_part = match last_colon {
            Some(idx) => &token[idx + 1..],
            None => token,
        };

        // Arbitrary VARIANT syntax uses double brackets: `word-[[…]]:utility`.
        // Check only the value_part so that variant prefixes containing `[[`
        // (e.g. `hover:has-[[data-checked]]:w-[42px]`) do not hide real values.
        if value_part.contains("[[") {
            continue;
        }

        let bracket_start = match value_part.find('[') {
            Some(i) => i,
            None => continue,
        };
        let bracket_end = match value_part.rfind(']') {
            Some(i) => i,
            None => continue,
        };
        if bracket_start >= bracket_end {
            continue;
        }
        let inner = &value_part[bracket_start + 1..bracket_end];

        // CSS custom properties and function compositions are legitimate uses.
        if CSS_FN_MARKERS.iter().any(|marker| inner.contains(marker)) {
            continue;
        }

        // Multi-property compound lists (e.g. `transition-[top,left,right,bottom,transform]`)
        // cannot be expressed as a single Tailwind utility.
        if inner.contains(',') {
            continue;
        }

        // Canonical CSS units with no Tailwind token equivalent are legitimate.
        if is_canonical_unit_value(inner) {
            continue;
        }

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

    #[test]
    fn ignores_var_composition() {
        assert!(!has_arbitrary_value("rounded-[var(--radius)]"));
        assert!(!has_arbitrary_value("w-[var(--sidebar-width)]"));
    }

    #[test]
    fn ignores_arbitrary_variant_double_bracket() {
        assert!(!has_arbitrary_value(
            "in-[[data-slot=item][data-checked]]:opacity-100"
        ));
        assert!(!has_arbitrary_value("has-[[data-checked]]:bg-blue-500"));
        assert!(!has_arbitrary_value("not-[[data-disabled]]:cursor-pointer"));
    }

    #[test]
    fn ignores_canonical_units() {
        assert!(!has_arbitrary_value("max-w-[30ch]"));
        assert!(!has_arbitrary_value("leading-[1.5lh]"));
        assert!(!has_arbitrary_value("w-[10cap]"));
        assert!(!has_arbitrary_value("w-[50cqi]"));
        assert!(!has_arbitrary_value("h-[25cqb]"));
    }

    #[test]
    fn ignores_function_compositions() {
        assert!(!has_arbitrary_value(
            "bg-[radial-gradient(circle_at_top,oklch(from_var(--color-primary)_calc(l+0.1)_c_h)_0%,transparent_70%)]"
        ));
        assert!(!has_arbitrary_value("w-[calc(100%-2rem)]"));
        assert!(!has_arbitrary_value("text-[clamp(1rem,2vw,2rem)]"));
        assert!(!has_arbitrary_value("bg-[oklch(0.5_0.2_200)]"));
    }

    // Regression for #261: a viewport-conditional ceiling mixing a viewport
    // unit with a fixed length cannot be expressed by a single token.
    #[test]
    fn ignores_viewport_conditional_min_max() {
        assert!(!has_arbitrary_value("max-w-[min(90vw,32rem)]"));
        assert!(!has_arbitrary_value("w-[max(50vw,20rem)]"));
    }

    #[test]
    fn still_flags_magic_numbers() {
        assert!(has_arbitrary_value("bg-[#abc]"));
        assert!(has_arbitrary_value("w-[42px]"));
        assert!(has_arbitrary_value("p-[16px]"));
        assert!(has_arbitrary_value("text-[#fff]"));
    }

    // Fix 1: arbitrary variant in prefix must not hide real value on right side
    #[test]
    fn arbitrary_variant_prefix_does_not_hide_real_value() {
        assert!(has_arbitrary_value("hover:has-[[data-checked]]:w-[42px]"));
        assert!(!has_arbitrary_value(
            "in-[[data-slot=item][data-checked]]:opacity-100"
        ));
        assert!(!has_arbitrary_value("data-[[checked]]:bg-red-500"));
    }

    // Fix 2: multi-dot numeric must be rejected
    #[test]
    fn multi_dot_numeric_is_rejected() {
        assert!(!is_canonical_unit_value("1.2.3ch"));
    }

    // Regression #486: percentage arbitrary values have no token equivalent
    #[test]
    fn ignores_percentage_values() {
        assert!(!has_arbitrary_value("before:h-[200%]"));
        assert!(!has_arbitrary_value("w-[50%]"));
        assert!(!has_arbitrary_value("h-[150%]"));
    }

    // Regression #486: Tailwind v4 --theme() function is a CSS function
    #[test]
    fn ignores_theme_function() {
        assert!(!has_arbitrary_value(
            "before:shadow-[0_1px_--theme(--color-black/4%)]"
        ));
    }

    // Regression #486: compound transition property lists cannot be a single utility
    #[test]
    fn ignores_compound_property_list() {
        assert!(!has_arbitrary_value(
            "transition-[top,left,right,bottom,transform]"
        ));
    }

    // Fix 4: modern responsive-viewport units
    #[test]
    fn ignores_responsive_viewport_units() {
        assert!(!has_arbitrary_value("h-[100dvh]"));
        assert!(!has_arbitrary_value("h-[100svh]"));
        assert!(!has_arbitrary_value("w-[100dvw]"));
        assert!(!has_arbitrary_value("w-[100svw]"));
        assert!(!has_arbitrary_value("h-[100lvh]"));
        assert!(!has_arbitrary_value("w-[100lvw]"));
        assert!(!has_arbitrary_value("leading-[1.5rlh]"));
    }
}
