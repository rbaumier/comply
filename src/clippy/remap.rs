//! Remap table: clippy's reported lint code → comply's RuleMeta.
//!
//! Unlike oxlint (which decorates rule ids with plugin prefixes),
//! clippy reports lints with their canonical name — `clippy::unwrap_used`,
//! `clippy::too_many_arguments`, etc. — which is exactly the string we
//! store in `Backend::Clippy { lint }`. So the remap is a direct
//! HashMap with no string surgery.
//!
//! The `missing_docs` rustc lint is the only exception: it's not in the
//! `clippy::` namespace, but it serves the same role as the doc-coverage
//! rule, so we accept it as a binding key without prefix.

use std::collections::HashMap;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;

/// Build a lookup table from clippy's reported lint code to the comply
/// RuleMeta that owns it. The binding key is used verbatim — clippy
/// emits exactly the name you pass to `-W`.
pub fn build_table(
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
) -> HashMap<String, &'static RuleMeta> {
    let mut table = HashMap::with_capacity(bindings.len());
    for (lint, meta, _) in bindings {
        table.insert((*lint).to_string(), *meta);
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_table_indexes_by_clippy_lint_name() {
        const META: RuleMeta = RuleMeta {
            id: "rust-no-unwrap",
            description: "no unwrap",
            remediation: "use ?",
            severity: Severity::Error,
            doc_url: None,
            categories: &[],
            skip_in_test_dir: false,
            skip_in_relaxed_dir: false,
        };
        let bindings: Vec<(&'static str, &'static RuleMeta, Severity)> =
            vec![("clippy::unwrap_used", &META, Severity::Error)];
        let table = build_table(&bindings);
        assert_eq!(
            table.get("clippy::unwrap_used").unwrap().id,
            "rust-no-unwrap"
        );
        assert!(!table.contains_key("clippy::other"));
    }
}
