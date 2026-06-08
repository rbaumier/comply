//! html-no-obsolete-tags — flag obsolete HTML tags and attributes that should
//! be replaced with CSS.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "html-no-obsolete-tags",
    description: "Obsolete HTML tags (center, font, marquee, blink, strike, big, tt) and presentational attributes (align, bgcolor, border on non-table elements) should be replaced by CSS.",
    remediation: "Use CSS instead of obsolete HTML tags",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["html"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
