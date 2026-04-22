//! ts-explicit-module-boundary-types — require explicit types on the
//! arguments and return values of exported functions.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-explicit-module-boundary-types",
    description: "Require explicit return and argument types on exported functions and class methods.",
    remediation: "Annotate every parameter and the return type of any exported \
                  function. Exported signatures are the module's public contract — \
                  inferred types drift silently as the implementation changes and \
                  surprise downstream consumers.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/explicit-module-boundary-types/"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
