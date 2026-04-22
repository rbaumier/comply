//! no-import-type-side-effects — ports typescript-eslint's
//! `@typescript-eslint/no-import-type-side-effects`: when every specifier
//! in an import has `type`, hoist it to `import type { ... }` so the
//! runtime module is not even fetched.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-import-type-side-effects",
    description: "Every specifier is a `type` import — hoist to a top-level `import type`.",
    remediation: "Replace `import { type A, type B } from 'x'` with `import type { A, B } from 'x'` \
                  so the bundler can skip the runtime module entirely under \
                  `verbatimModuleSyntax`.",
    severity: Severity::Warning,
    doc_url: Some("https://typescript-eslint.io/rules/no-import-type-side-effects"),
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
