//! dockerignore-must-exclude-sensitive — placeholder check: when the
//! Dockerfile ships broad `COPY .`, warn that `.dockerignore` must list
//! sensitive files. A filesystem-level read of `.dockerignore` is not yet
//! wired, so for now the rule surfaces the risk on the Dockerfile side.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerignore-must-exclude-sensitive",
    description: "When Dockerfile uses `COPY .`, `.dockerignore` must exclude `.env`, `.git`, `node_modules`, keys, etc.",
    remediation: "Create or extend `.dockerignore` with `.env*`, `.git`, `node_modules`, `*.pem`, `id_rsa`, `.npmrc`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Dockerfile, Backend::Text(Box::new(text::Check)))],
    }
}
