//! Flags `await` expressions inside the body of a collection/range loop —
//! `for…of`, `for…in`, or a C-style `for(;;)` — which iterate a known set
//! and could be parallelized with `Promise.all(items.map(f))`.
//!
//! `while`/`do…while` loops are exempt: they have no iteration set to map
//! over, only a runtime condition re-evaluated each pass, so they are
//! inherently sequential control flow (polling, retry, queue-drain) that
//! cannot be rewritten as `Promise.all`.
//!
//! Within the flaggable loops, `await` is also exempt when the awaited call
//! is a recursive call to the enclosing async function (sequential recursion
//! is a legitimate pattern for ordered traversals), or when the loop is a
//! retry/polling loop — one that exits early on a result (`return`/`break`)
//! and paces itself with a delay/backoff `await`
//! (`delay`/`sleep`/`setTimeout`).

mod oxc_typescript;
#[cfg(test)]
mod typescript;

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
