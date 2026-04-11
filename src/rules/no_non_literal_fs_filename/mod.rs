//! no-non-literal-fs-filename

mod typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-non-literal-fs-filename",
    description: "Filesystem operations with non-literal filenames can lead to path traversal attacks.",
    remediation: "Use string literals for filenames, or validate / sanitize the path before passing it to `fs` methods.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family_with_rust!(META, typescript, rust)
}
