//! type-only-dependency — flag production deps used only via `import type`.
//!
//! When every import of an npm package is `import type`, the package is
//! erased at build time. Declaring it under `dependencies` ships it as a
//! runtime requirement for consumers (and for production installs with
//! `--omit=dev`), even though no runtime code path needs it. Moving it to
//! `devDependencies` reflects reality and trims the install footprint.
//!
//! `@types/*` packages are skipped — they live in devDependencies by
//! convention and TypeScript already resolves them as ambient types, so
//! flagging them adds no signal.

mod text;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "type-only-dependency",
    description: "Production dependency is only imported via `import type` — move to devDependencies.",
    remediation: "Move the package from `dependencies` to `devDependencies` since it's only used for type information at build time.",
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
