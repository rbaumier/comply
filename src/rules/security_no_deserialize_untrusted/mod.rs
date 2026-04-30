//! security-no-deserialize-untrusted

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "security-no-deserialize-untrusted",
    description: "Unsafe deserializers (`unserialize`, `deserialize`, `yaml.load`, `pickle.loads`) fed with user input allow RCE.",
    remediation: "Use safe parsers: `JSON.parse`, `yaml.safeLoad` / `yaml.load` with `FAILSAFE_SCHEMA`, or validate/whitelist the data before deserializing.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
