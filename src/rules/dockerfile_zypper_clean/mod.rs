mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "dockerfile-zypper-clean",
    description: "`zypper install` must be paired with `zypper clean` to shrink the layer.",
    remediation: "Append `&& zypper clean` to the same RUN as `zypper install`.",
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
