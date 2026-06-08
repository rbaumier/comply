//! eslint-plugin-promise rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        entry(
            "promise/catch-or-return",
            "promise/catch-or-return",
            "Every promise chain must end with `.catch(...)` or be returned \
             so the caller can handle rejection. Unhandled rejections crash.",
        ),
        entry(
            "promise/always-return",
            "promise/always-return",
            "Every `.then(cb)` must return a value. Returning nothing breaks \
             chain composition.",
        ),
        entry(
            "promise/no-multiple-resolved",
            "promise/no-multiple-resolved",
            "Don't call resolve/reject more than once in a Promise executor. \
             Only the first call has effect; the rest silently vanishes.",
        ),
        entry(
            "promise/no-nesting",
            "promise/no-nesting",
            "Don't nest `.then()` inside `.then()`. Flatten via await or \
             return the inner promise from the outer callback.",
        ),
        entry(
            "promise/no-return-wrap",
            "promise/no-return-wrap",
            "Don't `return Promise.resolve(x)` inside `.then()` — just \
             `return x`. The then-chain already wraps non-promise values.",
        ),
        entry(
            "promise/prefer-await-to-then",
            "promise/prefer-await-to-then",
            "Use `await` instead of `.then()` chains. await keeps control \
             flow linear and enables try/catch.",
        ),
        entry(
            "promise/no-return-in-finally",
            "promise/no-return-in-finally",
            "Don't `return` from `.finally()`. The return value is discarded \
             and misleads readers about the chain's result.",
        ),
        entry(
            "promise/param-names",
            "promise/param-names",
            "Name Promise executor parameters `resolve` and `reject`. Any \
             other names confuse reviewers.",
        ),
    ]
}

// Entry-builder helper used by `register_all` above.

fn entry(id: &'static str, oxlint_key: &'static str, remediation: &'static str) -> RuleDef {
    oxlint_delegate(
        RuleMeta {
            id,
            description: "Promise discipline — avoid classic async footguns.",
            remediation,
            severity: Severity::Error,
            doc_url: None,
            categories: &["typescript"],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        },
        oxlint_key,
        TS_FAMILY,
    )
}
