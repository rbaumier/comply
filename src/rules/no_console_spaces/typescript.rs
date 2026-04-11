//! no-console-spaces — flag leading/trailing spaces in console method
//! string arguments that produce misaligned output.
//!
//! Matches `call_expression` nodes where the callee is `console.*`,
//! then inspects each string literal argument for leading/trailing
//! single spaces when the argument is not first/last.

use crate::diagnostic::{Diagnostic, Severity};

const CONSOLE_METHODS: &[&str] = &["log", "debug", "info", "warn", "error"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(obj) = callee.child_by_field_name("object") else { return };
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    let Ok(prop_text) = prop.utf8_text(source) else { return };

    if obj_text != "console" || !CONSOLE_METHODS.contains(&prop_text) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    if arg_count < 2 {
        return; // single arg cannot have misleading spaces
    }

    for i in 0..arg_count {
        let Some(arg) = args.named_child(i) else { continue };
        if arg.kind() != "string" && arg.kind() != "template_string" {
            continue;
        }
        let Ok(raw) = arg.utf8_text(source) else { continue };
        // Strip outer quotes
        let val = if raw.len() >= 2 { &raw[1..raw.len()-1] } else { continue };
        if val.is_empty() {
            continue;
        }

        let is_first = i == 0;
        let is_last = i == arg_count - 1;

        // Leading single space in non-first arg
        if !is_first && val.len() > 1 && val.starts_with(' ') && !val.starts_with("  ") {
            let pos = arg.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-console-spaces".into(),
                message: "Do not use leading space between `console` parameters.".into(),
                severity: Severity::Warning,
            });
        }

        // Trailing single space in non-last arg
        if !is_last && val.len() > 1 && val.ends_with(' ') && !val.ends_with("  ") {
            let pos = arg.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-console-spaces".into(),
                message: "Do not use trailing space between `console` parameters.".into(),
                severity: Severity::Warning,
            });
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
    fn flags_trailing_space_in_first_arg() {
        let d = run_on(r#"console.log("val: ", x);"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("trailing"));
    }

    #[test]
    fn flags_leading_space_in_last_arg() {
        let d = run_on(r#"console.log(x, " val");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("leading"));
    }

    #[test]
    fn allows_no_spaces() {
        assert!(run_on(r#"console.log("hello", x);"#).is_empty());
    }

    #[test]
    fn allows_single_arg_with_trailing_space() {
        assert!(run_on(r#"console.log("hello ");"#).is_empty());
    }

    #[test]
    fn allows_multiple_spaces() {
        assert!(run_on(r#"console.log("  hello", x);"#).is_empty());
    }
}
