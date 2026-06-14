//! no-hardcoded-secret — scan for committed API keys / tokens.

mod rust;
pub(crate) mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-hardcoded-secret",
    description: "Hardcoded secrets get exfiltrated from source control.",
    remediation: "Move the secret to an environment variable or secret \
                  store. Rotate the secret immediately — assume it is \
                  already compromised if it reached a commit.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    let mut backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Vue,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    // Rust uses a tree-sitter backend so it can skip credentials inside
    // `#[cfg(test)]` modules, which the directory-based test lever can't reach.
    backends.push((Language::Rust, Backend::TreeSitter(Box::new(rust::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}
