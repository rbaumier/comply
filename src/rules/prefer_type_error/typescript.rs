//! prefer-type-error backend — flag `throw new Error()` in type-checking conditions.
//!
//! When an `if` statement's condition is a type check (typeof, instanceof,
//! Array.isArray, etc.) and the only statement in the body is `throw new Error()`,
//! it should be `throw new TypeError()` instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Names of functions commonly used for type checking (on member expressions).
const TYPE_CHECK_IDENTIFIERS: &[&str] = &[
    "isArguments",
    "isArray",
    "isArrayBuffer",
    "isArrayLike",
    "isArrayLikeObject",
    "isBigInt",
    "isBoolean",
    "isBuffer",
    "isDate",
    "isElement",
    "isError",
    "isFinite",
    "isFunction",
    "isInteger",
    "isLength",
    "isMap",
    "isNaN",
    "isNative",
    "isNil",
    "isNull",
    "isNumber",
    "isObject",
    "isObjectLike",
    "isPlainObject",
    "isPrototypeOf",
    "isRegExp",
    "isSafeInteger",
    "isSet",
    "isString",
    "isSymbol",
    "isTypedArray",
    "isUndefined",
    "isView",
    "isWeakMap",
    "isWeakSet",
    "isWindow",
    "isXMLDoc",
];

/// Global type-check identifiers (can be called without a receiver).
const TYPE_CHECK_GLOBALS: &[&str] = &["isNaN", "isFinite"];

/// Returns true if the name matches an Error constructor pattern.
fn is_error_constructor_name(name: &str) -> bool {
    name.ends_with("Error") && name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// Returns true if the given node represents a type-checking expression.
fn is_typechecking_expression(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" => {
            // A bare identifier is only a type check if it's a global
            // type-check function being called — but at the identifier level
            // we can't tell if it's a callee. We handle this at call_expression.
            false
        }
        "call_expression" => {
            let Some(callee) = node.child_by_field_name("function") else {
                return false;
            };
            let Some(args) = node.child_by_field_name("arguments") else {
                return false;
            };
            if args.named_child_count() == 0 {
                return false;
            }
            match callee.kind() {
                "identifier" => {
                    let name = callee.utf8_text(source).unwrap_or("");
                    TYPE_CHECK_GLOBALS.contains(&name)
                }
                "member_expression" => is_typecheck_member_expression(callee, source),
                _ => false,
            }
        }
        "unary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .map(|o| o.utf8_text(source).unwrap_or(""))
                .unwrap_or("");
            if op == "typeof" {
                return true;
            }
            if op == "!"
                && let Some(arg) = node.child_by_field_name("argument")
            {
                return is_typechecking_expression(arg, source);
            }
            false
        }
        "binary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .map(|o| o.utf8_text(source).unwrap_or(""))
                .unwrap_or("");
            if op == "instanceof" {
                // `x instanceof Error` is a type check, but if the right side
                // is an Error constructor, we should still flag — the original
                // rule skips if right is an Error constructor.
                let right = node.child_by_field_name("right");
                if let Some(r) = right {
                    let rtext = r.utf8_text(source).unwrap_or("");
                    if is_error_constructor_name(rtext) {
                        return false;
                    }
                    // member_expression: check property
                    if r.kind() == "member_expression"
                        && let Some(prop) = r.child_by_field_name("property")
                        && is_error_constructor_name(prop.utf8_text(source).unwrap_or(""))
                    {
                        return false;
                    }
                }
                return true;
            }
            // typeof x === 'string' or similar — check both sides.
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");
            let left_check = left.is_some_and(|l| is_typechecking_expression(l, source));
            let right_check = right.is_some_and(|r| is_typechecking_expression(r, source));
            left_check || right_check
        }
        "parenthesized_expression" => {
            // Unwrap parentheses.
            node.named_child(0)
                .is_some_and(|inner| is_typechecking_expression(inner, source))
        }
        _ => false,
    }
}

/// Check if a member expression chain contains a type-check identifier.
fn is_typecheck_member_expression(node: tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(prop) = node.child_by_field_name("property") {
        let name = prop.utf8_text(source).unwrap_or("");
        if TYPE_CHECK_IDENTIFIERS.contains(&name) {
            return true;
        }
    }
    // Recurse into the object if it's also a member expression.
    if let Some(obj) = node.child_by_field_name("object")
        && obj.kind() == "member_expression"
    {
        return is_typecheck_member_expression(obj, source);
    }
    false
}

/// Returns true if an if_statement's body has exactly one statement.
fn body_has_single_statement(body: tree_sitter::Node, source: &[u8]) -> bool {
    let _ = source;
    match body.kind() {
        "statement_block" => body.named_child_count() == 1,
        // Single statement without braces.
        _ => true,
    }
}

/// Extract the single throw statement from an if body, if it exists.
#[allow(dead_code)]
fn get_single_throw(body: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match body.kind() {
        "statement_block" => {
            if body.named_child_count() != 1 {
                return None;
            }
            let child = body.named_child(0)?;
            if child.kind() == "throw_statement" {
                Some(child)
            } else {
                None
            }
        }
        "throw_statement" => Some(body),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["throw_statement"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();

        // The thrown value must be `new Error(...)`.
        let thrown = match node.named_child(0) {
            Some(c) => c,
            None => return,
        };
        if thrown.kind() != "new_expression" {
            return;
        }
        let Some(ctor) = thrown.child_by_field_name("constructor") else {
            return;
        };
        let ctor_name = ctor.utf8_text(source).unwrap_or("");
        // Only flag `new Error(...)`, not `new TypeError(...)` etc.
        if ctor_name != "Error" {
            return;
        }

        // The throw must be the lone statement in its parent block.
        let parent = match node.parent() {
            Some(p) => p,
            None => return,
        };
        if !body_has_single_statement(parent, source) {
            return;
        }

        // The parent block must be the consequence of an if_statement.
        let if_node = match parent.parent() {
            Some(p) if p.kind() == "if_statement" => p,
            _ => {
                // Also handle: throw is directly the consequence (no braces).
                if let Some(gp) = parent.parent()
                    && gp.kind() == "if_statement"
                {
                    // parent is the throw itself, and gp is if_statement.
                    // But we're already at the throw level, so check
                    // if throw's parent is if_statement.
                }
                // Check if the throw is directly under if_statement consequence.
                if parent.kind() == "if_statement" {
                    parent
                } else {
                    return;
                }
            }
        };

        // Check that the if condition is a type-checking expression.
        let Some(condition) = if_node.child_by_field_name("condition") else {
            return;
        };

        // The condition node in tree-sitter is a parenthesized_expression.
        let cond_inner = if condition.kind() == "parenthesized_expression" {
            condition.named_child(0).unwrap_or(condition)
        } else {
            condition
        };

        if !is_typechecking_expression(cond_inner, source) {
            return;
        }

        let pos = ctor.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-type-error".into(),
            message: "`new Error()` is too unspecific for a type check. \
                      Use `new TypeError()` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_typeof_check_with_error() {
        let code = r#"if (typeof x !== 'string') { throw new Error('bad'); }"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("TypeError"));
    }

    #[test]
    fn flags_instanceof_check_with_error() {
        let code = r#"if (!(x instanceof Foo)) { throw new Error('bad'); }"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_isnan_check() {
        let code = r#"if (isNaN(x)) { throw new Error('not a number'); }"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_array_isarray_check() {
        let code = r#"if (!Array.isArray(x)) { throw new Error('expected array'); }"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_typeerror_already() {
        let code = r#"if (typeof x !== 'string') { throw new TypeError('bad'); }"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_non_type_check_condition() {
        let code = r#"if (x > 10) { throw new Error('too big'); }"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_multiple_statements_in_body() {
        let code = r#"if (typeof x !== 'string') { console.log('bad'); throw new Error('bad'); }"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_instanceof_error_check() {
        // instanceof Error on the right side — this is checking FOR an error, not a type check.
        let code = r#"if (!(x instanceof Error)) { throw new Error('bad'); }"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_non_error_throw() {
        let code = r#"if (typeof x !== 'string') { throw new RangeError('bad'); }"#;
        assert!(run_on(code).is_empty());
    }
}
