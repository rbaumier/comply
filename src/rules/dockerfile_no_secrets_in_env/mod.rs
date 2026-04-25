//! dockerfile-no-secrets-in-env — ENV baked into the image layer is readable
//! forever; secrets must come from runtime env or `--mount=type=secret`.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-secrets-in-env",
    description: "ENV must not embed secret values; they persist in every image layer.",
    remediation: "Inject secrets at runtime or via `RUN --mount=type=secret`; never `ENV API_KEY=...`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Dockerfile, Backend::TreeSitter(Box::new(typescript::Check)))],
    }
}
