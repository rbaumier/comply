//! prefer-destructuring-assignment backend — flag consecutive `const x = obj.prop` accesses.

use crate::diagnostic::{Diagnostic, Severity};

/// If the node is a `lexical_declaration` of form `const/let VAR = OBJ.prop;`,
/// return the object name text. Otherwise None.
fn extract_object_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "lexical_declaration" {
        return None;
    }
    // Must have exactly one declarator
    let declarator = node.named_child(0)?;
    if declarator.kind() != "variable_declarator" {
        return None;
    }
    let value = declarator.child_by_field_name("value")?;
    if value.kind() != "member_expression" {
        return None;
    }
    let obj = value.child_by_field_name("object")?;
    // Object must be a simple identifier
    if obj.kind() != "identifier" {
        return None;
    }
    // Property must not be a method call — the parent (value) is member_expression,
    // and it should NOT be the function child of a call_expression.
    let parent_of_decl = node.parent()?;
    // Check that value isn't used as a call (the member_expression is just property access)
    // We check the sibling: if the lexical_declaration's value node's parent is a call, skip.
    // Actually, member_expression as value of variable_declarator can't be a call.
    // But we need to exclude method calls like `obj.getX()`.
    // In that case the value would be a call_expression, not a member_expression. So we're safe.
    let _ = parent_of_decl;

    obj.utf8_text(source).ok()
}

crate::ast_check! { on ["statement_block", "program"] => |node, source, ctx, diagnostics|
    // We look at statement blocks / program to find consecutive declarations
    let child_count = node.named_child_count();
    let mut i = 0;
    while i < child_count {
        let child = node.named_child(i).unwrap();
        if let Some(obj_name) = extract_object_name(child, source) {
            let _start = i;
            let mut count = 1usize;
            let mut j = i + 1;
            while j < child_count {
                let next = node.named_child(j).unwrap();
                if let Some(next_obj) = extract_object_name(next, source)
                    && next_obj == obj_name {
                        count += 1;
                        j += 1;
                        continue;
                    }
                break;
            }
            if count >= 2 {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "prefer-destructuring-assignment".into(),
                    message: format!(
                        "{count} consecutive property accesses on `{obj_name}` — use destructuring instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                i = j;
                continue;
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_consecutive_accesses() {
        let src = "const x = obj.x;\nconst y = obj.y;";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("obj"));
    }

    #[test]
    fn flags_three_consecutive() {
        let src = "const a = config.a;\nconst b = config.b;\nconst c = config.c;";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3"));
    }

    #[test]
    fn allows_single_access() {
        assert!(run_on("const x = obj.x;").is_empty());
    }

    #[test]
    fn allows_different_objects() {
        let src = "const x = obj1.x;\nconst y = obj2.y;";
        assert!(run_on(src).is_empty());
    }
}
