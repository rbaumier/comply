//! no-index-file — discourage `index.{js,ts,tsx,jsx,mjs}` barrel files.
//!
//! Barrel files cause bundler bloat, circular-import hazards, and make
//! "go to definition" harder. Rule only fires when the index file has
//! re-exports (`export * from ...`, `export { ... } from ...`) — plain
//! `index.ts` that holds implementation code is fine.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-index-file",
    description: "Barrel `index.*` files re-exporting from siblings cause bundler bloat \
                  and circular-import risk.",
    remediation: "Drop the barrel. Import directly from the module that defines the symbol \
                  (`import { foo } from './foo'`, not `from '.'`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["style"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
