//! no-section-divider-comments — flag ASCII section dividers in comments.
//!
//! Decorative comment dividers like `// ===========`, `// ***** SETUP *****`,
//! or `// ---- helpers ----` signal that one file is doing several different
//! things. The fix isn't a fancier divider — it's splitting the file by
//! responsibility. The coding-standards skill: "A file doing two things is
//! two files."

mod oxc_typescript;
mod rust;
mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-section-divider-comments",
    description: "ASCII section dividers signal a file doing too many things.",
    remediation: "Remove the divider and split the file by responsibility — each \
                  section becomes its own module. Section dividers in code are a \
                  hack around the real problem: the file should be smaller.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

const DIVIDER_CHARS: &[u8] = b"=-*#~";

/// True if the comment text contains a run of divider characters of at
/// least `min_run` length. Walks the entire raw comment text — leading
/// markers (`//`, `/*`) contain no divider characters so they don't
/// inflate the run count.
pub(crate) fn is_section_divider_text(text: &str, min_run: usize) -> bool {
    let mut longest: usize = 0;
    let mut current: usize = 0;
    let mut last: u8 = 0;
    for &b in text.as_bytes() {
        if DIVIDER_CHARS.contains(&b) {
            if b == last {
                current += 1;
            } else {
                current = 1;
                last = b;
            }
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
            last = 0;
        }
    }
    longest >= min_run
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
