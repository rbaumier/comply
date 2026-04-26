mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-no-zypper-dist-upgrade",
    description: "`zypper dist-upgrade` (or `dup`) is non-deterministic and bloats images.",
    remediation: "Remove `zypper dist-upgrade`/`dup`; install only the packages you need.",
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
