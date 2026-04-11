//! no-useless-error-capture-stack-trace backend — flag unnecessary
//! `Error.captureStackTrace(this, ClassName)` in Error subclass constructors.
//!
//! Built-in Error subclasses capture the stack trace automatically via
//! `super()`, so calling `Error.captureStackTrace(this, ClassName)` in the
//! constructor is redundant.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BUILTIN_ERRORS: &[&str] = &[
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
    "SuppressedError",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            // We look for class declarations/expressions that extend a builtin Error.
            if node.kind() != "class_declaration" && node.kind() != "class" {
                return;
            }

            // Check superclass is a builtin Error.
            let super_name = {
                let mut found = None;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "class_heritage" {
                        let text = child.utf8_text(source).unwrap_or("");
                        if let Some(rest) = text.strip_prefix("extends") {
                            let name = rest
                                .trim()
                                .split(|c: char| !c.is_alphanumeric())
                                .next()
                                .unwrap_or("");
                            if BUILTIN_ERRORS.contains(&name) {
                                found = Some(name.to_string());
                            }
                        }
                    }
                }
                found
            };
            if super_name.is_none() {
                return;
            }

            // Get class name (for matching the second argument).
            let class_name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            // Find the constructor.
            let Some(body) = node.child_by_field_name("body") else {
                return;
            };
            let mut body_cursor = body.walk();
            for member in body.children(&mut body_cursor) {
                if member.kind() != "method_definition" {
                    continue;
                }
                let Some(name_node) = member.child_by_field_name("name") else {
                    continue;
                };
                if name_node.utf8_text(source).unwrap_or("") != "constructor" {
                    continue;
                }

                // Found the constructor — now walk its body for
                // `Error.captureStackTrace(this, ClassName)` calls.
                let Some(func_body) = member.child_by_field_name("body") else {
                    continue;
                };
                let block = if func_body.kind() == "statement_block" {
                    func_body
                } else if let Some(b) = func_body.child_by_field_name("body") {
                    b
                } else {
                    continue;
                };

                let mut stmt_cursor = block.walk();
                for stmt in block.children(&mut stmt_cursor) {
                    if stmt.kind() != "expression_statement" {
                        continue;
                    }
                    let Some(expr) = stmt.named_child(0) else {
                        continue;
                    };
                    if expr.kind() != "call_expression" {
                        continue;
                    }

                    // Check callee is `Error.captureStackTrace`.
                    let Some(callee) = expr.child_by_field_name("function") else {
                        continue;
                    };
                    if callee.kind() != "member_expression" {
                        continue;
                    }
                    let Some(obj) = callee.child_by_field_name("object") else {
                        continue;
                    };
                    let Some(prop) = callee.child_by_field_name("property") else {
                        continue;
                    };
                    if obj.utf8_text(source).unwrap_or("") != "Error" {
                        continue;
                    }
                    if prop.utf8_text(source).unwrap_or("") != "captureStackTrace" {
                        continue;
                    }

                    // Check arguments: (this, ClassName) or (this, new.target) or (this, this.constructor).
                    let Some(args) = expr.child_by_field_name("arguments") else {
                        continue;
                    };
                    if args.named_child_count() != 2 {
                        continue;
                    }
                    let Some(first_arg) = args.named_child(0) else {
                        continue;
                    };
                    let Some(second_arg) = args.named_child(1) else {
                        continue;
                    };

                    if first_arg.kind() != "this" {
                        continue;
                    }

                    let second_text = second_arg.utf8_text(source).unwrap_or("");
                    let is_class_ref = second_text == class_name
                        || second_text == "new.target"
                        || second_text == "this.constructor";

                    if !is_class_ref {
                        continue;
                    }

                    let pos = expr.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-useless-error-capture-stack-trace".into(),
                        message: "Unnecessary `Error.captureStackTrace()` call. \
                                  Built-in Error subclasses capture the stack \
                                  trace automatically via `super()`."
                            .into(),
                        severity: Severity::Warning,
                    });
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
    fn flags_capture_stack_trace_with_class_name() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super(message);
        Error.captureStackTrace(this, MyError);
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unnecessary"));
    }

    #[test]
    fn flags_capture_stack_trace_with_new_target() {
        let code = r#"
class MyError extends TypeError {
    constructor(message) {
        super(message);
        Error.captureStackTrace(this, new.target);
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_capture_stack_trace_with_this_constructor() {
        let code = r#"
class MyError extends RangeError {
    constructor(message) {
        super(message);
        Error.captureStackTrace(this, this.constructor);
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_error_subclass() {
        let code = r#"
class MyClass extends Base {
    constructor() {
        super();
        Error.captureStackTrace(this, MyClass);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_different_second_argument() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super(message);
        Error.captureStackTrace(this, SomeOtherClass);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_no_capture_stack_trace() {
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super(message);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_single_argument_capture() {
        // Only 1 argument — not the pattern we flag (needs 2).
        let code = r#"
class MyError extends Error {
    constructor(message) {
        super(message);
        Error.captureStackTrace(this);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }
}
