//! dockerfile-copy-after-install — copy lockfile + install before the rest of
//! the source, so layer caching survives unrelated edits.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-copy-after-install",
    description: "`COPY . .` must not precede the dependency install step; copy the lockfile and install first.",
    remediation: "Copy `package.json` + lockfile, run install, then `COPY . .` so source edits don't invalidate the deps layer.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker"],
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
