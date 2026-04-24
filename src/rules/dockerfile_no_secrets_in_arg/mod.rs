//! dockerfile-no-secrets-in-arg — `ARG SECRET=foo` leaks into image history;
//! use `--mount=type=secret` for build-time secrets.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-secrets-in-arg",
    description: "ARG must not carry secret defaults; they leak into image history.",
    remediation: "Remove the default value and source the secret via `RUN --mount=type=secret`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Dockerfile, Backend::Text(Box::new(text::Check)))],
    }
}
