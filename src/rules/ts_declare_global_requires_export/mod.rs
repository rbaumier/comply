//! ts-declare-global-requires-export — files with `declare global` need
//! a top-level `export {}` to be treated as modules.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "ts-declare-global-requires-export",
    description: "`declare global` only augments the global scope when the file is a module; needs at least `export {}`.",
    remediation: "Add `export {};` at the end of the file so TypeScript treats it as a module and the `declare global` block takes effect.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
