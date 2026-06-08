//! dockerfile-no-from-platform — pinning `--platform` on FROM defeats
//! BuildKit's multi-arch support and usually masks an emulation bug.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-from-platform",
    description: "Avoid pinning `--platform` on FROM; it breaks multi-arch builds.",
    remediation: "Remove `--platform=...` and let BuildKit pick the platform via build args.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(
            Language::Dockerfile,
            Backend::TreeSitter(Box::new(typescript::Check)),
        )],
    }
}
