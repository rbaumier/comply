mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-size-shorthand",
    description: "`w-X h-X` with equal values can be written as `size-X`.",
    remediation: "Replace `w-4 h-4` with `size-4` (Tailwind v3.4+).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

/// Return the matching `w-V`/`h-V` value if both appear in the class string.
pub(crate) fn find_wh_duplicate<'a>(class_str: &'a str) -> Option<&'a str> {
    let tokens: Vec<&str> = class_str.split_whitespace().collect();
    let w_vals: Vec<&str> = tokens.iter().filter_map(|t| t.strip_prefix("w-")).collect();
    let h_vals: Vec<&str> = tokens.iter().filter_map(|t| t.strip_prefix("h-")).collect();
    w_vals.into_iter().find(|w| h_vals.contains(w))
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
