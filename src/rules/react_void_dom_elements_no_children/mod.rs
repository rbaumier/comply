//! react-void-dom-elements-no-children — void elements cannot have children.

mod text;
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "react-void-dom-elements-no-children",
    description: "Void HTML elements like `<br>`, `<img>`, `<input>` cannot have children.",
    remediation: "Remove children or `children`/`dangerouslySetInnerHTML` props \
                  from void elements. These elements are self-closing by spec — \
                  `<br />`, `<img />`, etc.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: {
            let mut b: Vec<_> = TS_FAMILY
                .iter()
                .map(|&lang| (lang, Backend::TreeSitter(Box::new(typescript::Check))))
                .collect();
            b.push((Language::Vue, Backend::Text(Box::new(text::Check))));
            b
        },
    }
}
