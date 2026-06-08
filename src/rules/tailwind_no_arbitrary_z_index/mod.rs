mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-arbitrary-z-index",
    description: "Arbitrary z-index values `z-[n]` bypass the design token scale.",
    remediation: "Use a design token (`z-10`, `z-50`) or define a custom token in `tailwind.config.ts`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// True when the class string contains a `z-[N…]` token whose first inner
/// character is an ASCII digit.
pub(crate) fn has_arbitrary_numeric_z(s: &str) -> bool {
    for token in s.split_whitespace() {
        if let Some(rest) = token.strip_prefix("z-[")
            && rest.starts_with(|c: char| c.is_ascii_digit())
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
