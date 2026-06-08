//! rust-no-pub-use-glob — never re-export with a wildcard.
//!
//! `pub use foo::*` re-exports every public symbol from `foo` into
//! the current module. The visible API of your crate then depends
//! on whatever happens to be `pub` in `foo` — adding a private
//! helper there silently widens your API, removing one silently
//! breaks downstream. There's no way to grep for "what does this
//! crate export" because the answer requires reading every dependency.
//!
//! List the re-exports explicitly: `pub use foo::{Bar, Baz};`. The
//! list itself becomes the API contract.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-pub-use-glob",
    description: "`pub use foo::*` re-exports invisibly.",
    remediation: "List the re-exports explicitly: `pub use foo::{Bar, Baz};`. \
                  Glob re-exports turn every change in `foo` into a silent \
                  change to your public API.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};
pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
