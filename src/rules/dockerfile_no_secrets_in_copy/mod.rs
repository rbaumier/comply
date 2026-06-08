//! dockerfile-no-secrets-in-copy — never `COPY` files that typically carry
//! credentials (`.env`, `*.pem`, `id_rsa`, `.npmrc`).

mod check;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-secrets-in-copy",
    description: "COPY must not include files that typically hold credentials (`.env`, `*.pem`, `id_rsa`, `.npmrc`).",
    remediation: "Add these paths to `.dockerignore` and inject secrets via `--mount=type=secret` at build time.",
    severity: Severity::Error,
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
            Backend::TreeSitter(Box::new(check::Check)),
        )],
    }
}
