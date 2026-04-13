//! custom-error-definition backend — flag `this.name = 'X'` in Error subclass constructors.
//!
//! Detects Error subclasses that set `this.name` in the constructor instead of
//! using a class field `name = 'ClassName'`. Also flags `this.message = ...`
//! assignments — the message should be passed to `super()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

/// Matches PascalCase names ending in "Error".
fn is_error_name(name: &str) -> bool {
    name.ends_with("Error") && name.starts_with(|c: char| c.is_ascii_uppercase())
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "class_declaration" && node.kind() != "class" {
                return;
            }

            // Must extend something that looks like an Error.
            let has_error_super = {
                let mut found = false;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "class_heritage" {
                        let heritage_text = child.utf8_text(source).unwrap_or("");
                        // Extract the superclass name from "extends ErrorName"
                        if let Some(rest) = heritage_text.strip_prefix("extends") {
                            let super_name = rest
                                .trim()
                                .split(|c: char| !c.is_alphanumeric())
                                .next()
                                .unwrap_or("");
                            if is_error_name(super_name) {
                                found = true;
                            }
                        }
                    }
                }
                found
            };

            if !has_error_super {
                return;
            }

            // Get class name.
            let class_name = match node.child_by_field_name("name") {
                Some(n) => n.utf8_text(source).unwrap_or(""),
                None => return,
            };

            // Get the class body.
            let Some(body) = node.child_by_field_name("body") else {
                return;
            };

            // Look for constructor and scan its body.
            let mut body_cursor = body.walk();
            for member in body.children(&mut body_cursor) {
                if member.kind() != "method_definition" {
                    continue;
                }
                // Check if it's a constructor.
                let name_node = match member.child_by_field_name("name") {
                    Some(n) => n,
                    None => continue,
                };
                if name_node.utf8_text(source).unwrap_or("") != "constructor" {
                    continue;
                }

                // Get the function body.
                let Some(func_body) = member.child_by_field_name("body") else {
                    continue;
                };
                let Some(block) = (if func_body.kind() == "statement_block" {
                    Some(func_body)
                } else {
                    func_body.child_by_field_name("body")
                }) else {
                    continue;
                };

                // Walk statements in the constructor body.
                let mut stmt_cursor = block.walk();
                for stmt in block.children(&mut stmt_cursor) {
                    if stmt.kind() != "expression_statement" {
                        continue;
                    }
                    let Some(expr) = stmt.named_child(0) else {
                        continue;
                    };
                    if expr.kind() != "assignment_expression" {
                        continue;
                    }
                    let Some(left) = expr.child_by_field_name("left") else {
                        continue;
                    };
                    let left_text = left.utf8_text(source).unwrap_or("");

                    // Flag `this.name = ...`
                    if left_text == "this.name" {
                        let pos = stmt.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "custom-error-definition".into(),
                            message: format!(
                                "Use a class field `name = '{class_name}';` instead \
                                 of setting `this.name` in the constructor."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }

                    // Flag `this.message = ...`
                    if left_text == "this.message" {
                        let pos = stmt.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "custom-error-definition".into(),
                            message: "Pass the error message to `super()` instead \
                                      of setting `this.message`."
                                .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_this_name_in_constructor() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super(message);
        this.name = 'MyError';
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class field"));
    }

    #[test]
    fn flags_this_message_in_constructor() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super();
        this.message = message;
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("super()"));
    }

    #[test]
    fn flags_both_name_and_message() {
        let code = r#"
class MyError extends Error {
    constructor(msg) {
        super();
        this.name = 'MyError';
        this.message = msg;
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_class_field_name() {
        let code = r#"
class MyError extends Error {
    name = 'MyError';
    constructor(message) {
        super(message);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_non_error_class() {
        let code = r#"
class MyThing extends Base {
    constructor() {
        super();
        this.name = 'MyThing';
    }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_empty_constructor() {
        let code = r#"
class MyError extends Error {
    name = 'MyError';
    constructor(msg) {
        super(msg);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }
}
