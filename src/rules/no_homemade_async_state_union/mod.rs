//! no-homemade-async-state-union — a request lifecycle spelled out by hand,
//! beside the one the request library already returns.
//!
//! ## What is flagged
//!
//! - A string-literal union holding at least two async phase names, one of
//!   which is purely async: `type Status = "idle" | "loading"`, a
//!   `useState<"loading" | "error">()` type argument, a `status:` field.
//! - An object type, an `interface`, or a grouped `useState` whose fields are
//!   at least two of `data` / `loading` / `error` / `isLoading` / `isPending` /
//!   `isError` / `isSuccess`, carrying both a boolean `loading` / `isLoading`
//!   and an `error` / `isError` channel — the hand-rolled
//!   `{ data, loading, error }` triplet.
//!
//! ## What is left alone
//!
//! - Domain state that happens to share a word: `"pending"` names an order and
//!   `"failed"` a payment, so the union has to carry one of the purely-async
//!   words (`loading` / `fetching` / `refetching`) before it fires.
//! - A type that comes from the library: `type X = QueryStatus` and a `status`
//!   prop typed from `@tanstack/react-query` are type references, not literal
//!   unions, so they never match.
//! - `{ data, error }` with no boolean flag — that is a `Result`, which is the
//!   shape the rule is pushing people towards.
//! - `{ data, isLoading }` with no failure channel — a presentational
//!   component's props, fed by a query it does not own.
//! - Test files (`skip_in_test_dir`); generated, minified and vendored files
//!   are dropped by the engine before any rule runs.
//! - Type aliases in a file that imports `@tanstack/react-query`:
//!   `react-no-request-state-mirror` already reports those with its own
//!   remediation, and one defect deserves one diagnostic.
//!
//! ## Configuration
//!
//! The four word lists live in `[rules.no-homemade-async-state-union]` of
//! `src/config/defaults.toml` and are read through
//! [`crate::rules::async_state_helpers`], which shares them with
//! `react-no-request-state-mirror`.

mod oxc_typescript;
mod oxc_vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-homemade-async-state-union",
    description: "Async state re-modelled by hand as a literal union or a `{ data, loading, error }` object.",
    remediation: "Homemade async state — read `status` / `isPending` / `error` \
                  from the query result (TanStack Query, SWR) or return a \
                  `Result`; no parallel state machine.",
    severity: Severity::Error,
    doc_url: Some("https://tanstack.com/query/latest/docs/framework/react/guides/queries"),
    categories: &["typescript", "async"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(oxc_vue::Check))),
        ],
    }
}
