//! Flags `await` expressions inside loop bodies, except when the awaited
//! call is a recursive call to the enclosing async function (sequential
//! recursion is a legitimate pattern for ordered traversals).

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-await-in-loop",
    description: "Sequential `await` in a loop serializes independent work.",
    remediation: "If the iterations don't depend on each other, use \
                  `Promise.all(items.map(f))` instead. If they do depend, keep the \
                  loop and document why. Recursive calls to the enclosing async \
                  function are exempt — depth-first traversal must stay sequential.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
