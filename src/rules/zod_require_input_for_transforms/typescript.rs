//! zod-require-input-for-transforms backend — flag `z.infer<typeof X>` where
//! `X`'s declaration contains a `.transform(` call.

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};

fn find_const_init<'a>(root: Node<'a>, source: &'a [u8], name: &str) -> Option<Node<'a>> {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "variable_declarator"
            && let Some(name_node) = n.child_by_field_name("name")
                && name_node.utf8_text(source).map(|t| t == name).unwrap_or(false) {
                    return n.child_by_field_name("value");
                }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
    None
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Find every generic_type that looks like `z.infer<typeof X>`.
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "generic_type" {
            let text = n.utf8_text(source).unwrap_or("");
            if text.starts_with("z.infer<") || text.starts_with("z.infer <") {
                // Extract the inner `typeof X` identifier.
                // Pattern: generic_type -> type_arguments -> type_query -> identifier
                if let Some(args) = n.child_by_field_name("type_arguments") {
                    let mut ac = args.walk();
                    for arg in args.named_children(&mut ac) {
                        if arg.kind() == "type_query" {
                            let mut qc = arg.walk();
                            for q in arg.named_children(&mut qc) {
                                if q.kind() == "identifier" {
                                    let Ok(schema_name) = q.utf8_text(source) else { continue };
                                    if let Some(init) = find_const_init(node, source, schema_name) {
                                        let init_text = init.utf8_text(source).unwrap_or("");
                                        if init_text.contains(".transform(") {
                                            let pos = n.start_position();
                                            diagnostics.push(Diagnostic {
                                                path: std::sync::Arc::clone(&ctx.path_arc),
                                                line: pos.row + 1,
                                                column: pos.column + 1,
                                                rule_id: super::META.id.into(),
                                                message: format!(
                                                    "`{schema_name}` uses `.transform()` — \
                                                     `z.infer` returns the transformed *output* type. \
                                                     Use `z.input<typeof {schema_name}>` for form values."
                                                ),
                                                severity: Severity::Warning,
                                                span: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_infer_on_transformed_schema() {
        let src = "const S = z.string().transform(v => v.trim());\n\
                   type T = z.infer<typeof S>;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_input_on_transformed_schema() {
        let src = "const S = z.string().transform(v => v.trim());\n\
                   type T = z.input<typeof S>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_infer_without_transform() {
        let src = "const S = z.object({ a: z.string() });\n\
                   type T = z.infer<typeof S>;";
        assert!(run(src).is_empty());
    }
}
