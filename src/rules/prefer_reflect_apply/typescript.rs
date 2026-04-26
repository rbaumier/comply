//! prefer-reflect-apply backend — flag `fn.apply()` in favor of `Reflect.apply()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    let Some(property) = func.child_by_field_name("property") else { return };
    let method_name = property.utf8_text(source).unwrap_or("");

    if method_name != "apply" { return; }

    // Check if this is already `Reflect.apply(…)`.
    let Some(object) = func.child_by_field_name("object") else { return };
    if object.kind() == "identifier" && object.utf8_text(source).unwrap_or("") == "Reflect" {
        return;
    }

    // Check for `Function.prototype.apply.call(…)` pattern.
    // In that case `object` is `Function.prototype.apply` and method is `call`.
    // We actually want to catch `.apply(` on anything except `Reflect`.

    let pos = node.start_position();

    // Check if it's the `Function.prototype.apply.call(…)` pattern.
    let full_text = func.utf8_text(source).unwrap_or("");
    if full_text.contains("Function.prototype.apply.call") {
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-reflect-apply".into(),
            message: "Prefer `Reflect.apply(fn, thisArg, args)` over `Function.prototype.apply.call(fn, thisArg, args)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }

    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-reflect-apply".into(),
        message: "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.".into(),
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
    fn flags_direct_apply() {
        let d = run_on("fn.apply(null, args);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-reflect-apply");
    }

    #[test]
    fn flags_apply_with_this() {
        let d = run_on("foo.bar.apply(this, args);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_reflect_apply() {
        assert!(run_on("Reflect.apply(fn, null, args);").is_empty());
    }

    #[test]
    fn allows_non_apply_method() {
        assert!(run_on("fn.call(null, args);").is_empty());
    }
}
