//! vue-no-script-url

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-no-script-url",
    description: "`javascript:` URLs in Vue template attributes are an XSS vector.",
    remediation: "Use a `@click` handler instead of a `javascript:` URL.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
