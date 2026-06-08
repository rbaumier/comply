//! compose-bind-localhost-ports — DB/cache ports must bind to `127.0.0.1:`
//! so dev boxes don't accidentally expose them on the LAN.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-bind-localhost-ports",
    description: "Database/cache service ports must bind on `127.0.0.1:`.",
    remediation: "Prefix the published port with `127.0.0.1:`, e.g. `127.0.0.1:5432:5432`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["docker", "docker-compose"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
