//! ts-method-signature-style — enforce property signature for methods in interfaces.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-method-signature-style",
    description: "Shorthand method signatures in interfaces are less safe than property signatures — they allow unsafe variance.",
    remediation: "Use a property signature with a function type: `foo: (x: string) => void` instead of `foo(x: string): void`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/method-signature-style"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
