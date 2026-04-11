//! rust-large-enum-variant — box large variants so the enum stays small.

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-large-enum-variant",
    description: "Enum size equals the largest variant — box big variants.",
    remediation: "Wrap the large variant's payload in `Box<T>` so the enum \
                  stays small. Otherwise every instance of the enum — even \
                  the small-variant case — pays the full size cost. Enable \
                  `clippy::large_enum_variant`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Rust,
            Backend::Clippy { lint: "clippy::large_enum_variant" },
        )],
    }
}
