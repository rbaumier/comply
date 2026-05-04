mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-shorthand",
    description: "Collapse redundant Tailwind utility pairs into their shorthand form (e.g. `px-2 py-2` → `p-2`).",
    remediation: "Replace pairs like `pt-N pb-N` with `py-N`, `pl-N pr-N` with `px-N`, and `px-N py-N` with `p-N`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

/// Pair of prefixes that can collapse into one shorthand when their value matches.
pub(crate) const SHORTHAND_PAIRS: &[(&str, &str, &str)] = &[
    ("px-", "py-", "p-"),
    ("pt-", "pb-", "py-"),
    ("pl-", "pr-", "px-"),
    ("mx-", "my-", "m-"),
    ("mt-", "mb-", "my-"),
    ("ml-", "mr-", "mx-"),
    ("top-", "bottom-", "inset-y-"),
    ("left-", "right-", "inset-x-"),
    ("scroll-px-", "scroll-py-", "scroll-p-"),
    ("scroll-pt-", "scroll-pb-", "scroll-py-"),
    ("scroll-pl-", "scroll-pr-", "scroll-px-"),
    ("scroll-mx-", "scroll-my-", "scroll-m-"),
    ("scroll-mt-", "scroll-mb-", "scroll-my-"),
    ("scroll-ml-", "scroll-mr-", "scroll-mx-"),
    ("rounded-t-", "rounded-b-", "rounded-y-"),
    ("rounded-l-", "rounded-r-", "rounded-x-"),
    ("w-", "h-", "size-"),
];

pub(crate) fn split_variant(class: &str) -> (&str, &str) {
    match class.rfind(':') {
        Some(idx) => (&class[..=idx], &class[idx + 1..]),
        None => ("", class),
    }
}

pub(crate) fn strip_important(class: &str) -> (bool, &str) {
    match class.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, class),
    }
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
