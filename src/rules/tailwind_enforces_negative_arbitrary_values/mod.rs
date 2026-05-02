mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-enforces-negative-arbitrary-values",
    description: "Negative arbitrary Tailwind values should live inside the brackets, not on the utility prefix.",
    remediation: "Use top-[-1px] instead of -top-[1px]",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub(crate) const NEGATABLE_PROPS: &[&str] = &[
    "top", "bottom", "left", "right",
    "m", "mt", "mb", "ml", "mr", "mx", "my",
    "p", "pt", "pb", "pl", "pr", "px", "py",
    "inset", "translate", "rotate", "skew", "scale",
];

/// Returns `true` if `token` matches the shape `-<prop>-[<value>…]` where
/// `<prop>` is in `NEGATABLE_PROPS` and `<value>` does NOT itself start
/// with `-`.
pub(crate) fn is_negative_prefix_arbitrary(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('-') else {
        return false;
    };
    for prop in NEGATABLE_PROPS {
        let needle = format!("{prop}-[");
        if let Some(after_bracket) = rest.strip_prefix(&needle)
            && !after_bracket.is_empty()
            && !after_bracket.starts_with('-')
            && after_bracket.contains(']')
        {
            return true;
        }
    }
    false
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Vue,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
