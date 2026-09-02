//! cargo-release-profile-lto — the default `release` profile compiles each
//! crate as its own codegen unit with no cross-crate inlining. Enabling
//! link-time optimization and collapsing to a single codegen unit is the
//! cheapest whole-program speed-up available: no code changes, only a longer
//! release build.
//!
//! The rule fires once on the root `Cargo.toml` — the workspace manifest, or a
//! standalone package manifest with no manifest above it — when
//! `[profile.release]` is missing, or is present without `lto` and
//! `codegen-units = 1`. Member manifests are never flagged: Cargo reads
//! profiles from the workspace root only.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "cargo-release-profile-lto",
    description: "Root `Cargo.toml` ships a release profile without link-time optimization or a single codegen unit.",
    remediation: "Add to the root `Cargo.toml`:\n\
                  \n\
                  [profile.release]\n\
                  lto = \"fat\"\n\
                  codegen-units = 1\n\
                  \n\
                  `lto = \"thin\"` is the cheaper compromise when release build time matters. \
                  `panic = \"abort\"` is a separate decision — it skips `Drop` on panic, so RAII \
                  guards and tracing flushes never run; enable it deliberately, this rule does not \
                  require it.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["rust", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Toml, Backend::Text(Box::new(text::Check)))],
    }
}
