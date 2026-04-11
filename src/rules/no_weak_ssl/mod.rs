//! no-weak-ssl

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-weak-ssl",
    description: "Weak SSL/TLS protocol versions are insecure.",
    remediation: "Use TLSv1.2 or TLSv1.3. Older protocols (SSLv2, SSLv3, TLSv1.0, TLSv1.1) have known vulnerabilities.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
