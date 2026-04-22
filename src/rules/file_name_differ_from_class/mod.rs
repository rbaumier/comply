//! file-name-differ-from-class (SonarJS S3317)
//!
//! Flags modules whose sole primary export (class or function declaration)
//! doesn't match the file name. Matching is case-insensitive and tolerates
//! any of kebab-case, snake_case, camelCase and PascalCase as long as the
//! alphanumeric letter sequence is the same as the exported binding's.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "file-name-differ-from-class",
    description: "A file exporting a single class or function should be named after it.",
    remediation: "Rename the file to match the exported symbol (PascalCase, camelCase, kebab-case or snake_case are accepted).",
    severity: Severity::Warning,
    doc_url: Some("https://sonarsource.github.io/rspec/#/rspec/S3317"),
    categories: &["code-quality"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
