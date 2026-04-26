use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["unary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else {
        return;
    };
    let op_text = &source[op_node.byte_range()];
    if op_text != b"!" {
        return;
    }

    let Some(arg) = node.child_by_field_name("argument") else {
        return;
    };
    if arg.kind() != "parenthesized_expression" {
        return;
    }

    let mut cursor = arg.walk();
    for child in arg.children(&mut cursor) {
        if child.kind() != "binary_expression" {
            continue;
        }
        let Some(bin_op) = child.child_by_field_name("operator") else {
            continue;
        };
        let bin_op_text = &source[bin_op.byte_range()];
        if bin_op_text != b"&&" && bin_op_text != b"||" {
            continue;
        }
        let pos = node.start_position();
        let op_str = std::str::from_utf8(bin_op_text).unwrap_or("??");
        let suggested = if op_str == "&&" { "||" } else { "&&" };
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "de-morgan-simplify".into(),
            message: format!(
                "Apply De Morgan's law: `!(a {op_str} b)` simplifies to `!a {suggested} !b`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source), &tree)
    }

    #[test]
    fn flags_negated_and() {
        let d = run("if (!(a && b)) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a || !b"));
    }

    #[test]
    fn flags_negated_or() {
        let d = run("if (!(a || b)) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("!a && !b"));
    }

    #[test]
    fn allows_simple_negation() {
        assert!(run("if (!a) {}").is_empty());
    }

    #[test]
    fn allows_negated_comparison() {
        assert!(run("if (!(a === b)) {}").is_empty());
    }
}
