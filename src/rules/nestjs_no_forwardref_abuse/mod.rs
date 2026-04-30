//! nestjs-no-forwardref-abuse — `forwardRef` masks circular dependencies.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-forwardref-abuse",
    description: "`forwardRef(() => ...)` papers over circular module/provider dependencies.",
    remediation: "Restructure the dependency graph: extract a shared interface or push the \
                  shared type into a third module both sides depend on.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Text(Box::new(typescript::Check)),
            ),
            (Language::Tsx, Backend::Text(Box::new(typescript::Check))),
        ],
    }
}
