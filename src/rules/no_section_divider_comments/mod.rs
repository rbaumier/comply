//! no-section-divider-comments — flag ASCII section dividers in comments.
//!
//! Decorative comment dividers like `// ===========`, `// ***** SETUP *****`,
//! or `// ---- helpers ----` signal that one file is doing several different
//! things. The fix isn't a fancier divider — it's splitting the file by
//! responsibility. The coding-standards skill: "A file doing two things is
//! two files."

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-section-divider-comments",
    description: "ASCII section dividers signal a file doing too many things.",
    remediation: "Remove the divider and split the file by responsibility — each \
                  section becomes its own module. Section dividers in code are a \
                  hack around the real problem: the file should be smaller.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],
};

pub fn register() -> RuleDef {
    let backends: Vec<_> = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Rust,
    ]
    .into_iter()
    .map(|lang| (lang, Backend::Text(Box::new(text::Check))))
    .collect();
    RuleDef { meta: META, backends }
}
