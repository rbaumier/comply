//! ui-antialiased-on-root — root/html/body should enable
//! `-webkit-font-smoothing: antialiased` so text renders crisply on macOS.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-antialiased-on-root",
    description: "The root element should set `-webkit-font-smoothing: antialiased` for crisp text on macOS/iOS.",
    remediation: "Add `-webkit-font-smoothing: antialiased;` to `html`, `body`, or `:root`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["ui"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Css, Backend::TreeSitter(Box::new(text::Check)))],
    }
}
