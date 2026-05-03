//! security-no-deserialize-untrusted

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
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
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
