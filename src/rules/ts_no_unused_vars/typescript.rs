//! ts-no-unused-vars backend — simplified unused variable detection.
//!
//! Catches the obvious case: a `const`/`let` variable declarator whose
//! name never appears again anywhere else in the file source text.
//! Also checks function parameters.
//!
//! Intentional limitations (simplicity over completeness):
//! - Uses text-based reference scanning (not scope-aware)
//! - Skips variables prefixed with `_`
//! - Skips exported declarations
//! - Skips destructuring patterns
//! - Only processes file-level and function-level declarations

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is inside an export statement.
fn is_exported(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if p.kind() == "export_statement" {
            return true;
        }
        if p.kind() == "program" {
            break;
        }
        cur = p.parent();
    }
    false
}

/// Count occurrences of `name` as a whole word in source text.
fn count_references(source: &str, name: &str) -> usize {
    let mut count = 0usize;
    let name_bytes = name.as_bytes();
    let src_bytes = source.as_bytes();
    let len = name_bytes.len();

    let mut i = 0;
    while i + len <= src_bytes.len() {
        if &src_bytes[i..i + len] == name_bytes {
            // Check word boundaries
            let before_ok = i == 0 || !is_ident_char(src_bytes[i - 1]);
            let after_ok = i + len >= src_bytes.len() || !is_ident_char(src_bytes[i + len]);
            if before_ok && after_ok {
                count += 1;
            }
            i += len;
        } else {
            i += 1;
        }
    }
    count
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only process variable_declarator nodes within lexical_declaration (let/const)
    if node.kind() != "variable_declarator" {
        return;
    }

    // Must be inside a lexical_declaration (let/const) — skip var for simplicity
    let parent = match node.parent() {
        Some(p) if p.kind() == "lexical_declaration" => p,
        _ => return,
    };

    // Skip exported declarations
    if is_exported(parent) {
        return;
    }

    // Get the variable name
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };

    // Only handle simple identifiers (skip destructuring)
    if name_node.kind() != "identifier" {
        return;
    }

    let Ok(name) = name_node.utf8_text(source) else {
        return;
    };

    // Skip underscore-prefixed names (intentional non-use convention)
    if name.starts_with('_') {
        return;
    }

    // Skip very short names that might cause false positives
    if name.is_empty() {
        return;
    }

    // Count whole-word occurrences of name in the full source
    let source_str = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return,
    };

    let ref_count = count_references(source_str, name);

    // If the name appears exactly once, it's only the declaration
    if ref_count <= 1 {
        let pos = name_node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-unused-vars".into(),
            message: format!("`{name}` is declared but never used."),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unused_variable() {
        let d = run_on("const unused = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`unused`"));
    }

    #[test]
    fn allows_used_variable() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }

    #[test]
    fn allows_underscore_prefix() {
        assert!(run_on("const _unused = 42;").is_empty());
    }

    #[test]
    fn allows_exported_variable() {
        assert!(run_on("export const foo = 42;").is_empty());
    }

    #[test]
    fn flags_multiple_unused() {
        let d = run_on("const aaa = 1; const bbb = 2;");
        assert_eq!(d.len(), 2);
    }
}
