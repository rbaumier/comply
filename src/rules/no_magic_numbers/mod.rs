//! no-magic-numbers — ports typescript-eslint's `@typescript-eslint/no-magic-numbers`.
//! Flag unexplained numeric literals; allow a small set of common
//! constants (-1, 0, 1, 2) and any number extracted into a named binding.

mod rust;
mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-magic-numbers",
    description: "Numeric literal used inline instead of a named constant.",
    remediation: "Extract the number into a `const` with a descriptive name so its meaning \
                  is visible at the call site.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-magic-numbers"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
