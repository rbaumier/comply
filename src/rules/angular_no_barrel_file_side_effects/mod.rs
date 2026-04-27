//! angular-no-barrel-file-side-effects — barrel `index.ts` should only re-export.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-barrel-file-side-effects",
    description: "Barrel files should be pure re-exports — side effects break tree-shaking.",
    remediation: "Move statements out of `index.ts`; keep only `export {…} from …`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
