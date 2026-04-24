//! ui-no-display-none-exit — `display: none` as the only exit treatment
//! cannot be transitioned; pair it with opacity+translate.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ui-no-display-none-exit",
    description: "`display: none` can't be animated; a class/state that toggles it blocks the exit transition.",
    remediation: "Pair `display: none` with `opacity: 0` and `transform: translate...` (or use `visibility`) so the exit can be animated.",
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
