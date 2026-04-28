//! react-no-array-index-key — use stable ids, not indices.

mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-array-index-key",
    description: "Array indices as React keys break on reorder.",
    remediation: "Use a stable id from the data as the React key. \
                  `items.map(item => <X key={item.id} />)` instead of \
                  `items.map((item, i) => <X key={i} />)`. Index keys \
                  associate DOM state with the wrong item on reorder/filter.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::Tsx, Backend::TreeSitter(Box::new(react::Check))),
        ],
    }
}
