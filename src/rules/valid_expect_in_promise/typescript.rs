//! valid-expect-in-promise backend — flag `.then()`/`.catch()` calls that
//! contain `expect()` assertions in their callback but are neither returned
//! nor awaited.
//!
//! Why: without a `return` or `await`, the test function finishes before
//! the async callback runs, so the assertion — pass or fail — is silently
//! ignored and the test reports green for the wrong reason.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return;
    };
    let prop_bytes = &source[prop.byte_range()];
    if prop_bytes != b"then" && prop_bytes != b"catch" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    if !args_contain_expect(args, source) {
        return;
    }

    if is_returned_or_awaited(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "valid-expect-in-promise".into(),
        message: format!(
            "Promise `.{}()` with `expect()` inside must be returned or awaited.",
            std::str::from_utf8(prop_bytes).unwrap_or("then")
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// True if any descendant of `args` is a call to `expect(...)`.
fn args_contain_expect(args: Node<'_>, source: &[u8]) -> bool {
    let mut cursor = args.walk();
    let mut stack: Vec<Node<'_>> = args.named_children(&mut cursor).collect();
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function") {
                let name_bytes = match callee.kind() {
                    "identifier" => Some(&source[callee.byte_range()]),
                    "member_expression" => callee
                        .child_by_field_name("object")
                        .map(|o| &source[o.byte_range()]),
                    _ => None,
                };
                if name_bytes == Some(b"expect".as_slice()) {
                    return true;
                }
                // Also catch `expect(x).resolves.toBe(...)` style where the
                // call's function is a member chain rooted on `expect(...)`.
                if callee.kind() == "member_expression"
                    && member_root_is_expect_call(callee, source)
                {
                    return true;
                }
            }
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            stack.push(child);
        }
    }
    false
}

/// Walk down the `object` chain of a member_expression and return true if
/// the root is `expect(...)`.
fn member_root_is_expect_call(mut member: Node<'_>, source: &[u8]) -> bool {
    loop {
        let Some(obj) = member.child_by_field_name("object") else {
            return false;
        };
        match obj.kind() {
            "member_expression" => member = obj,
            "call_expression" => {
                let Some(fun) = obj.child_by_field_name("function") else {
                    return false;
                };
                return fun.kind() == "identifier" && &source[fun.byte_range()] == b"expect";
            }
            _ => return false,
        }
    }
}

/// True if `call` is the expression of a `return` statement, the operand
/// of an `await`, or the expression body of an arrow function.
fn is_returned_or_awaited(call: Node<'_>) -> bool {
    let mut current = call;
    loop {
        let Some(parent) = current.parent() else {
            return false;
        };
        match parent.kind() {
            "return_statement" | "await_expression" => return true,
            "arrow_function" => {
                // Only counts if `current` is the expression body, not a
                // parameter. Arrow body field is "body".
                if let Some(body) = parent.child_by_field_name("body") {
                    return body.id() == current.id();
                }
                return false;
            }
            // Transparent wrappers — keep climbing.
            "parenthesized_expression"
            | "non_null_expression"
            | "as_expression"
            | "satisfies_expression"
            | "type_assertion" => {
                current = parent;
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unhandled_then_with_expect() {
        let src = r#"
it('test', () => {
  promise.then(val => {
    expect(val).toBe(1);
  });
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_returned_then() {
        let src = r#"
it('test', () => {
  return promise.then(val => {
    expect(val).toBe(1);
  });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_awaited_then() {
        let src = r#"
it('test', async () => {
  await promise.then(val => {
    expect(val).toBe(1);
  });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_then_without_expect() {
        let src = r#"
it('test', () => {
  promise.then(val => {
    console.log(val);
  });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_catch_with_expect() {
        let src = r#"
it('test', () => {
  promise.catch(err => {
    expect(err).toBeDefined();
  });
});
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
