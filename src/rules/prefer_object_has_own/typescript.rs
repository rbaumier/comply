use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return; };
    if func.kind() != "member_expression" { return; }

    let Some(prop) = func.child_by_field_name("property") else { return; };
    let prop_name = prop.utf8_text(source).unwrap_or("");

    if prop_name != "hasOwnProperty" { return; }

    // Check it's not already Object.prototype.hasOwnProperty.call (allowed pattern)
    let Some(obj) = func.child_by_field_name("object") else { return; };
    let obj_text = obj.utf8_text(source).unwrap_or("");
    if obj_text == "Object.prototype" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-object-has-own".into(),
        message: "Use `Object.hasOwn(obj, key)` instead of `obj.hasOwnProperty(key)` (ES2022).".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_has_own_property() {
        assert_eq!(run("obj.hasOwnProperty('key')").len(), 1);
    }

    #[test]
    fn flags_this_has_own_property() {
        assert_eq!(run("this.hasOwnProperty('key')").len(), 1);
    }

    #[test]
    fn allows_object_has_own() {
        assert!(run("Object.hasOwn(obj, 'key')").is_empty());
    }

    #[test]
    fn allows_prototype_call() {
        assert!(run("Object.prototype.hasOwnProperty.call(obj, 'key')").is_empty());
    }
}
