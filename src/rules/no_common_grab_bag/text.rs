//! no-common-grab-bag backend — flag files named `common.ts`, `utils.ts`,
//! `helpers.ts`, `shared.ts`, `misc.ts`.
//!
//! These names are magnets for unrelated code — any function can land there
//! because the name doesn't mean anything. The skill rule: "Focused modules
//! — no `common`/`shared` grab-bags". Force the author to pick a meaningful
//! name describing what the module actually owns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const BANNED_STEMS: &[&str] = &["common", "utils", "helpers", "shared", "misc", "util"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(stem) = ctx.path.file_stem().and_then(|s| s.to_str()) else {
            return vec![];
        };
        let lower = stem.to_ascii_lowercase();
        if !BANNED_STEMS.contains(&lower.as_str()) {
            return vec![];
        }
        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: "no-common-grab-bag".into(),
            message: format!(
                "File '{lower}.*' is a grab-bag name — pick a name that \
                 describes what this module actually owns. `common`/`utils`/\
                 `helpers`/`shared`/`misc` magnetize unrelated code."
            ),
            severity: Severity::Warning,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path_str: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path_str), ""))
    }

    #[test]
    fn flags_utils_ts() {
        assert_eq!(run("src/utils.ts").len(), 1);
    }

    #[test]
    fn flags_common_js() {
        assert_eq!(run("src/common.js").len(), 1);
    }

    #[test]
    fn flags_shared_rs() {
        assert_eq!(run("src/shared.rs").len(), 1);
    }

    #[test]
    fn allows_meaningful_names() {
        for path in ["src/order_service.ts", "src/payment.ts", "src/auth.rs"] {
            assert!(run(path).is_empty(), "{path} should be allowed");
        }
    }
}
