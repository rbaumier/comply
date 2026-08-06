mod semver_range;
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-duplicate-dependencies",
    description: "A dependency is listed twice in one `package.json` section, or in two sections \
                  a package manager will not honour together — one listing is installed and the \
                  other ignored. A `dependencies` and `peerDependencies` pair is reported only \
                  when no version satisfies both declared ranges.",
    remediation: "Remove one of the listings so each dependency appears in exactly one section. \
                  For a `dependencies`/`peerDependencies` pair, instead widen or narrow one \
                  range so a single installed version satisfies both.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["suspicious", "package-json"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Json, Backend::Text(Box::new(text::Check)))],
    }
}
