//! angular-no-barrel-file-side-effects — barrel `index.ts` should only re-export.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-barrel-file-side-effects",
    description: "Barrel files should be pure re-exports — side effects break tree-shaking.",
    remediation: "Move statements out of `index.ts`; keep only `export {…} from …`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
