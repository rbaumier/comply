//! ts-no-use-before-define backend — simplified detection of variables
//! used before their `let`/`const` declaration in the same scope.
//!
//! Only checks top-level and function-level `let`/`const` declarations
//! (these are subject to the Temporal Dead Zone). Skips `var` (hoisted)
//! and function declarations (also hoisted).
//!
//! Approach: walk statement_block / program children in order. Track
//! which names have been declared. When we see an identifier reference
//! that matches a name declared *later* in the same block, flag it.

use std::collections::{HashMap, HashSet};
use crate::diagnostic::{Diagnostic, Severity};

/// Collect all identifier references in a node subtree (excluding declarations).
fn collect_refs(node: tree_sitter::Node, source: &[u8], refs: &mut Vec<(String, tree_sitter::Point)>) {
    if node.kind() == "identifier" {
        // Skip if this is the name of a declaration
        if let Some(parent) = node.parent() {
            if parent.kind() == "variable_declarator"
                && let Some(name_node) = parent.child_by_field_name("name")
                    && name_node.id() == node.id() {
                        return; // This is a declaration, not a reference
                    }
            // Skip property access identifiers (right side of `.`)
            if parent.kind() == "member_expression"
                && let Some(prop) = parent.child_by_field_name("property")
                    && prop.id() == node.id() {
                        return;
                    }
        }
        if let Ok(text) = node.utf8_text(source) {
            refs.push((text.to_string(), node.start_position()));
        }
        return;
    }

    // Don't recurse into nested scopes (functions create their own scope)
    if matches!(node.kind(), "function_declaration" | "function" | "arrow_function" | "class_declaration" | "class") {
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_refs(child, source, refs);
    }
}

fn check_block(
    block: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Phase 1: collect all let/const declarations and their byte positions
    let mut declarations: HashMap<String, usize> = HashMap::new(); // name -> byte_offset

    let mut cursor = block.walk();
    for child in block.named_children(&mut cursor) {
        if child.kind() == "lexical_declaration" {
            let mut inner = child.walk();
            for decl in child.named_children(&mut inner) {
                if decl.kind() == "variable_declarator"
                    && let Some(name_node) = decl.child_by_field_name("name")
                        && name_node.kind() == "identifier"
                            && let Ok(name) = name_node.utf8_text(source) {
                                declarations.entry(name.to_string())
                                    .or_insert(name_node.start_byte());
                            }
            }
        }
    }

    if declarations.is_empty() {
        return;
    }

    // Phase 2: collect all references in the block (excluding nested scopes)
    let mut already_flagged: HashSet<String> = HashSet::new();

    let mut cursor2 = block.walk();
    for child in block.named_children(&mut cursor2) {
        // For each statement, collect references
        let mut stmt_refs = Vec::new();
        collect_refs(child, source, &mut stmt_refs);

        for (name, pos) in stmt_refs {
            if already_flagged.contains(&name) {
                continue;
            }
            if let Some(&decl_offset) = declarations.get(&name) {
                // The reference is before the declaration
                let ref_start = child.start_byte();
                if ref_start < decl_offset {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "ts-no-use-before-define".into(),
                        message: format!("`{name}` is used before its definition."),
                        severity: Severity::Warning,
                        span: None,
                    });
                    already_flagged.insert(name);
                }
            }
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Process program and statement_block nodes
    match node.kind() {
        "program" | "statement_block" => {
            check_block(node, source, ctx, diagnostics);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_use_before_define() {
        let d = run_on("console.log(x); const x = 1;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_use_after_define() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }

    #[test]
    fn allows_function_declaration_hoisting() {
        // Function declarations are hoisted, so this should not flag.
        // We don't process var/function declarations.
        assert!(run_on("f(); function f() {}").is_empty());
    }
}
