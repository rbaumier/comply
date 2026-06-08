//! unused-dependency — flag npm dependencies declared in `package.json` that
//! no source file imports.
//!
//! A production dependency that's never imported is dead weight: it inflates
//! `node_modules`, slows installs, and lies about what the project actually
//! needs. The cross-file import index already collects every bare specifier
//! the codebase imports, so the check is a pure set difference against
//! `package.json#dependencies`.
//!
//! Tooling/framework packages used implicitly (e.g. `typescript`, `vite`,
//! `jest`) and `@types/*` packages are excluded — they're consumed by config,
//! not by `import` statements.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "unused-dependency",
    description: "Dependency in package.json is never imported in the project.",
    remediation: "Remove the dependency from package.json, or add an import if it's actually needed.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports", "dependencies"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Text(Box::new(text::Check))))
            .collect(),
    }
}
