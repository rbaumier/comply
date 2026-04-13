//! ts-no-import-type-side-effects backend — flag `import { type A, type B }`
//! where every specifier has an inline `type` qualifier but the import
//! itself lacks a top-level `type` keyword.
//!
//! Detection: walk `import_statement` nodes. If the import does NOT have
//! a top-level `type` keyword but every named import specifier has one,
//! report it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" {
        return;
    }
    // Check if import already has top-level `type`: `import type { ... }`
    // In tree-sitter-typescript, `import type` has "type" as a direct
    // child token right after `import`.
    let node_text = &source[node.byte_range()];
    let Ok(text) = std::str::from_utf8(node_text) else {
        return;
    };
    let trimmed = text.trim();
    // Quick check: if starts with "import type" it's already a type import
    if trimmed.starts_with("import type ") || trimmed.starts_with("import type{") {
        return;
    }
    // Find the import clause (named_imports)
    let mut cursor = node.walk();
    let mut named_imports_node = None;
    for child in node.named_children(&mut cursor) {
        if child.kind() == "import_clause" {
            let mut cc = child.walk();
            for clause_child in child.named_children(&mut cc) {
                if clause_child.kind() == "named_imports" {
                    named_imports_node = Some(clause_child);
                }
            }
        }
    }
    let Some(named_imports) = named_imports_node else {
        return;
    };
    // Gather all import specifiers
    let mut spec_cursor = named_imports.walk();
    let specifiers: Vec<_> = named_imports
        .named_children(&mut spec_cursor)
        .filter(|c| c.kind() == "import_specifier")
        .collect();
    if specifiers.is_empty() {
        return;
    }
    // Check every specifier has inline `type`
    let all_type = specifiers.iter().all(|spec| {
        let spec_text = &source[spec.byte_range()];
        if let Ok(s) = std::str::from_utf8(spec_text) {
            s.trim().starts_with("type ")
        } else {
            false
        }
    });
    if !all_type {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-import-type-side-effects".into(),
        message: "All specifiers have inline `type` qualifiers — use a \
                  top-level `import type` to avoid a runtime side-effect import."
            .into(),
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
    fn flags_all_inline_type_specifiers() {
        let diags = run_on("import { type A, type B } from 'mod';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_top_level_import_type() {
        assert!(run_on("import type { A, B } from 'mod';").is_empty());
    }

    #[test]
    fn allows_mixed_specifiers() {
        assert!(run_on("import { type A, B } from 'mod';").is_empty());
    }
}
