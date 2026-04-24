//! file-extension-in-import — require explicit file extensions on relative imports.
//!
//! Why: Node.js ESM requires explicit file extensions on relative import
//! specifiers. Omitting the extension relies on bundler/resolver magic that
//! does not exist in native ESM runtimes, producing `ERR_MODULE_NOT_FOUND`
//! at runtime. Making the extension explicit keeps the source portable
//! between bundlers, tsc, ts-node, Deno and native Node ESM.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "file-extension-in-import",
    description: "Relative imports should include a file extension for ESM compatibility.",
    remediation: "Add the appropriate file extension to the import path (e.g. `.js`, `.ts`).",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/eslint-community/eslint-plugin-n/blob/master/docs/rules/file-extension-in-import.md",
    ),
    categories: &["imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
