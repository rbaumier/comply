//! react-void-dom-elements-no-children — void elements cannot have children.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::backend::Backend;
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
            let b: Vec<_> = TS_FAMILY
                .iter()
                .map(|&lang| (lang, Backend::TreeSitter(Box::new(react::Check))))
                .collect();
            b
        },
    }
}
