mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-magic-spacing",
    description: "Arbitrary pixel spacing like `p-[13px]` breaks design-token consistency.",
    remediation: "Use the standard spacing scale (`p-1` = 4px, `p-2` = 8px, etc.) or arbitrary values that are multiples of 4.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub(crate) const SPACING_PREFIXES: &[&str] = &[
    "p-[", "px-[", "py-[", "pt-[", "pb-[", "pl-[", "pr-[",
    "m-[", "mx-[", "my-[", "mt-[", "mb-[", "ml-[", "mr-[",
    "gap-[", "gap-x-[", "gap-y-[", "space-x-[", "space-y-[",
];

/// Parse a value like `13px` as `Some(13)`. Anything that does not end in
/// `px` with only digits before it returns `None`.
pub(crate) fn parse_px(value: &str) -> Option<u64> {
    let stripped = value.strip_suffix("px")?;
    if stripped.is_empty() {
        return None;
    }
    stripped.parse::<u64>().ok()
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
