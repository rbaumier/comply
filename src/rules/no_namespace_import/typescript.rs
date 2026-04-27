//! no-namespace-import backend — flag `import * as …` patterns.
//!
//! Some packages legitimately require namespace imports because their
//! public API is a compound-component object (e.g. Radix UI's
//! `Dialog.Root` / `Dialog.Trigger`) or because a project conventionally
//! imports them that way (e.g. `import * as React from "react"`).
//! Sources matching [`NAMESPACE_ALLOWLIST`] are skipped.

use crate::diagnostic::{Diagnostic, Severity};

/// Package patterns where `import * as X from "…"` is the standard /
/// idiomatic usage. Each entry is either an exact module name or a
/// `prefix*` glob matching scoped families and their subpaths.
const NAMESPACE_ALLOWLIST: &[&str] = &[
    "react",
    "@radix-ui/*",
    "@headlessui/*",
    "@floating-ui/*",
    "d3",
    "d3-*",
    "three",
    "@react-three/*",
];

fn matches_allowlist(source: &str) -> bool {
    NAMESPACE_ALLOWLIST.iter().any(|pat| match pat.strip_suffix('*') {
        Some(prefix) => source.starts_with(prefix),
        None => source == *pat,
    })
}

fn import_source<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let src = node.child_by_field_name("source")?;
    let raw = src.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    if !text.contains("* as ") {
        return;
    }
    if let Some(src) = import_source(node, source) {
        if matches_allowlist(src) {
            return;
        }
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-namespace-import".into(),
        message: "Namespace import (`import * as …`) — prefer named imports.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_namespace_import() {
        let d = run_on("import * as utils from './utils';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Namespace import"));
    }

    #[test]
    fn allows_named_import() {
        assert!(run_on("import { foo, bar } from './utils';").is_empty());
    }

    #[test]
    fn allows_default_import() {
        assert!(run_on("import utils from './utils';").is_empty());
    }

    #[test]
    fn allows_radix_namespace_import() {
        assert!(run_on(r#"import * as Dialog from "@radix-ui/react-dialog";"#).is_empty());
        assert!(run_on(r#"import * as Tooltip from "@radix-ui/react-tooltip";"#).is_empty());
    }

    #[test]
    fn allows_react_namespace_import() {
        assert!(run_on(r#"import * as React from "react";"#).is_empty());
    }

    #[test]
    fn allows_headlessui_namespace_import() {
        assert!(run_on(r#"import * as Menu from "@headlessui/react";"#).is_empty());
    }

    #[test]
    fn allows_floating_ui_namespace_import() {
        assert!(run_on(r#"import * as FloatingUI from "@floating-ui/react";"#).is_empty());
    }

    #[test]
    fn allows_d3_namespace_import() {
        assert!(run_on(r#"import * as d3 from "d3";"#).is_empty());
        assert!(run_on(r#"import * as scale from "d3-scale";"#).is_empty());
    }

    #[test]
    fn allows_three_namespace_import() {
        assert!(run_on(r#"import * as THREE from "three";"#).is_empty());
        assert!(run_on(r#"import * as fiber from "@react-three/fiber";"#).is_empty());
    }

    #[test]
    fn flags_namespace_import_for_unrelated_package() {
        let d = run_on(r#"import * as everything from "some-random-lib";"#);
        assert_eq!(d.len(), 1);
    }
}
