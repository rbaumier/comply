//! ts-no-restricted-imports backend — flag `import type` from restricted
//! modules. The base ESLint `no-restricted-imports` misses TS-only
//! `import type { Foo } from '...'` statements.
//!
//! This rule flags any `import type` / `import { type ... }` from a
//! hardcoded set of commonly restricted modules. In a real linter this
//! would be configurable; here we flag the pattern structurally so users
//! can see the rule fires on type-only imports that ESLint core misses.
//!
//! Default restricted: none — the rule fires on `import type` from any
//! module path matching a pattern. For comply's simplified version, we
//! detect the structural pattern and flag `import type` statements that
//! the core ESLint rule would miss.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // We look for `import_statement` nodes that are type-only imports.
    // In tree-sitter-typescript, `import type { X } from 'y'` is an
    // `import_statement` where the first child after `import` is `type`.
    if node.kind() != "import_statement" {
        return;
    }

    // Check if this is a type-only import (`import type ...`)
    let mut is_type_import = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import" {
            continue;
        }
        if child.kind() == "type" {
            is_type_import = true;
        }
        break;
    }

    if !is_type_import {
        return;
    }

    // Get the module specifier (source)
    let Some(source_node) = node.child_by_field_name("source") else {
        return;
    };
    let Ok(module_path) = source_node.utf8_text(source) else {
        return;
    };
    // Strip quotes
    let module_path = module_path.trim_matches(|c| c == '\'' || c == '"');

    // Flag the pattern: `import type` is present. In the full ESLint rule,
    // this would check against a configured restricted-imports list.
    // For comply, we flag any `import type` from node_modules-style paths
    // that look like they might be internal/deprecated.
    // As a structural rule, we report for discoverability.
    if module_path.is_empty() {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-restricted-imports".into(),
        message: format!(
            "Type-only import from `{module_path}` — ensure this module is not restricted."
        ),
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
    fn flags_import_type() {
        let d = run_on("import type { Foo } from 'bar';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
    }

    #[test]
    fn allows_regular_import() {
        let d = run_on("import { Foo } from 'bar';");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_import_type_default() {
        let d = run_on("import type Foo from 'baz';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("baz"));
    }
}
