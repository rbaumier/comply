//! compose-no-network-host — `network_mode: host` bypasses Docker's
//! network isolation and must not appear in shipped compose files.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "compose-no-network-host",
    description: "Services must not set `network_mode: host`.",
    remediation: "Remove `network_mode: host`. Use a user-defined network with \
                  `ports:` mappings to expose only the ports you need. Host \
                  networking gives the container the daemon's network namespace, \
                  so every listening port on the container is reachable on the \
                  host with no firewalling.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["docker", "docker-compose", "security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Yaml, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
