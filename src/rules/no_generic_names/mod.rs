//! no-generic-names — reject vague/meaningless identifier names along three
//! axes: exact banned words matched on the whole identifier (`temp`, `result`,
//! `val`, `foo`, …); filler nouns matched as a word segment anywhere on a
//! camelCase/`_` boundary (`data` → `updatedData`, `getUserData`); and generic
//! verbs matched only as a leading prefix (`process`, `do`, `execute`, `run`,
//! `perform`). `handle` is excluded because `handleXxx` is a React idiom.
//!
//! A PascalCase binding whose initializer is a React component — a
//! `forwardRef`/`memo` call, an arrow/function returning JSX (including
//! `function Item() { return <li/>; }`), or a polymorphic element-type ternary
//! (`const Comp = asChild ? Slot : "div"`) — is exempt: `Input`/`Label`/`Comp`
//! are the conventional design-system / `asChild` component names, not generic
//! values. Other PascalCase bindings still flag (`const DefaultData =
//! computeData()`, `class DataSource {}`, `const Input = 5`).

mod oxc_typescript;
mod rust;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

/// Placeholder and filler identifiers that carry no meaning in any language.
/// Both backends ban these as a whole-identifier match; each adds its own
/// language-specific extras on top — see `oxc_typescript::EXTRA_BANNED_WORDS`
/// (type-ish and JS-idiom words: `str`, `ptr`, `entries`, `body`, …) and
/// `rust::EXTRA_BANNED_WORDS`. Keep this list to words that are generic in
/// every language: a word that is idiomatic in one language belongs in that
/// backend's extras, not here.
pub(super) const GENERIC_WORDS: &[&str] = &[
    "foo", "bar", "baz", "qux", "quux", "corge", "foobar", "blah", "bleh", "asdf", "qwerty", "zzz",
    "xxx", "aaa", "bbb", "scratch", "junk", "garbage", "dummy", "placeholder", "stub", "fake",
    "something", "anything", "whatever", "temp", "tmp", "result", "results", "retval", "ret",
    "val", "value", "values", "vars", "obj", "objs", "item", "items", "thing", "stuff", "info",
    "arr", "list", "lists", "rows", "payload", "payloads", "flag",
];

pub const META: RuleMeta = RuleMeta {
    id: "no-generic-names",
    description: "Generic names and mechanical prefixes carry no meaning.",
    remediation: "Rename to describe what the value IS or what the \
                  function accomplishes. `data` → `parsedOrder`, `temp` \
                  → name the actual intermediate, `processOrder` → \
                  `fulfillOrder`, `doPayment` → `chargeCustomer`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["naming"],

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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
        ],
    }
}
