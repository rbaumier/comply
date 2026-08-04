//! axum-no-body-on-get

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "axum-no-body-on-get",
    description: "A handler registered with `get`/`head` takes an extractor that requires a request content-type — every bodyless request is rejected before it runs.",
    remediation: "Read the input from `Query<…>`/`Path<…>` instead, or register \
                  the handler with `post`/`put` so the request may carry the \
                  body the extractor demands.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["correctness", "axum"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
