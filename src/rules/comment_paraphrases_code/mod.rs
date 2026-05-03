//! comment-paraphrases-code — flag comments that restate the code they sit on.

mod oxc_typescript;
mod rust;
mod text;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comment-paraphrases-code",
    description: "Comment shares too many tokens with the function name — likely a paraphrase.",
    remediation: "Rewrite the comment to explain WHY the code exists, not WHAT it does. \
                  Name the consequence: what breaks if this line is deleted? If you \
                  can't name a consequence, delete the comment instead.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
