//! unlisted-dependency — flag bare imports of npm packages that are not
//! declared in `package.json`.
//!
//! Mirrors the inverse of `unused-dependency`: the import index collects
//! every bare specifier the codebase pulls in, and this rule reports the
//! ones that are missing from any section of `package.json`. tsconfig path
//! aliases (`@/*`, `~/*`, …) are not packages — they're skipped by checking
//! the alias prefix list of the project's tsconfig.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "unlisted-dependency",
    description: "Import references an npm package not declared in package.json.",
    remediation: "Add the package to the appropriate section of package.json (dependencies or devDependencies).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["imports", "dependencies"],
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
