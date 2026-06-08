//! dockerfile-useradd-low-uid — `useradd` without `-l`/`--no-log-init` can
//! produce sparse `/var/log/lastlog` files that bloat the image. Hadolint DL3046.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-useradd-low-uid",
    description: "`useradd` without `-l` flag and high UID creates an excessively large image due to sparse `/var/log/lastlog`.",
    remediation: "Add `-l` flag to `useradd` or use a low UID (below 65534).",
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
