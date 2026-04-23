//! toml-no-mixed-type-in-array — forbid arrays whose elements have
//! heterogeneous types, which almost always signals a schema bug.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "toml-no-mixed-type-in-array",
    description: "TOML arrays should contain elements of a single type.",
    remediation: "Split the array into separate single-typed arrays, or \
                  convert the mixed values to a common type. TOML 1.0 permits \
                  mixed-type arrays, but most schemas (Cargo.toml, pyproject.toml, \
                  etc.) reject them — keeping arrays homogeneous avoids surprises.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["toml"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Toml, Backend::Text(Box::new(text::Check)))],
    }
}
