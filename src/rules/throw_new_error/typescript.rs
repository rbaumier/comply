//! throw-new-error backend — flag `throw Error('x')` without `new`.

use crate::diagnostic::{Diagnostic, Severity};

/// Matches PascalCase names ending in "Error": Error, TypeError, MyCustomError, etc.
fn is_error_like(name: &str) -> bool {
    if !name.ends_with("Error") || name.is_empty() {
        return false;
    }
    // Must start with uppercase.
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };

    let callee_name = match callee.kind() {
        // Direct call: `Error('x')`
        "identifier" => {
            let name = callee.utf8_text(source).unwrap_or("");
            if !is_error_like(name) { return; }
            name
        }
        // Member access: `module.CustomError('x')`
        "member_expression" => {
            let Some(prop) = callee.child_by_field_name("property") else { return };
            let name = prop.utf8_text(source).unwrap_or("");
            if !is_error_like(name) { return; }

            // Exclude Data.TaggedError (Effect library)
            let obj = callee.child_by_field_name("object");
            if let Some(obj_node) = obj {
                let obj_text = obj_node.utf8_text(source).unwrap_or("");
                if obj_text == "Data" && name == "TaggedError" {
                    return;
                }
            }
            name
        }
        _ => return,
    };

    // Check that the parent is a throw_statement — the rule is about
    // `throw Error(...)` specifically, but the original unicorn rule fires
    // on ANY call expression matching the pattern, not just throws.
    // We follow the original: flag all call expressions to Error-like
    // constructors without `new`.
    let _ = callee_name;

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "throw-new-error".into(),
        message: "Use `new` when creating an error.".into(),
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
    fn flags_error_without_new() {
        let d = run_on("throw Error('oops');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "throw-new-error");
    }

    #[test]
    fn flags_typeerror_without_new() {
        let d = run_on("throw TypeError('bad type');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_custom_error_without_new() {
        let d = run_on("throw MyCustomError('fail');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_new_error() {
        assert!(run_on("throw new Error('oops');").is_empty());
    }

    #[test]
    fn allows_new_typeerror() {
        assert!(run_on("throw new TypeError('bad');").is_empty());
    }

    #[test]
    fn allows_non_error_call() {
        assert!(run_on("console.log('hello');").is_empty());
    }

    #[test]
    fn allows_non_error_function() {
        assert!(run_on("foo();").is_empty());
    }

    #[test]
    fn flags_member_error_without_new() {
        let d = run_on("throw lib.CustomError('x');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_data_tagged_error() {
        // Effect library exception — Data.TaggedError is not an error constructor.
        assert!(run_on("Data.TaggedError('x');").is_empty());
    }

    #[test]
    fn allows_lowercase_function() {
        // `error()` is not PascalCase — not an error constructor.
        assert!(run_on("error('x');").is_empty());
    }
}
