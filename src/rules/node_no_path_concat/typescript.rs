//! node-no-path-concat backend — flag `__dirname + '...'` / `__filename + '...'`.

use crate::diagnostic::{Diagnostic, Severity};

const PATH_GLOBALS: &[&str] = &["__dirname", "__filename"];

crate::ast_check! { on ["binary_expression"] prefilter = ["__dirname", "__filename"] => |node, source, ctx, diagnostics|
    // Match binary expressions with `+` operator.
    let Some(op) = node.child_by_field_name("operator") else { return };
    if op.utf8_text(source).unwrap_or("") != "+" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else { return };
    let Some(right) = node.child_by_field_name("right") else { return };

    let left_is_path = left.kind() == "identifier"
        && PATH_GLOBALS.contains(&left.utf8_text(source).unwrap_or(""));
    let right_is_path = right.kind() == "identifier"
        && PATH_GLOBALS.contains(&right.utf8_text(source).unwrap_or(""));

    if !left_is_path && !right_is_path {
        return;
    }

    // Avoid double-reporting: if parent is also a `+` binary_expression and
    // we already matched a path global in it, skip.
    if let Some(parent) = node.parent()
        && parent.kind() == "binary_expression"
            && let Some(pop) = parent.child_by_field_name("operator")
                && pop.utf8_text(source).unwrap_or("") == "+" {
                    // Check if parent's left or right is a path global.
                    let parent_left = parent.child_by_field_name("left");
                    if let Some(pl) = parent_left
                        && pl.kind() == "identifier"
                            && PATH_GLOBALS.contains(&pl.utf8_text(source).unwrap_or(""))
                        {
                            return;
                        }
                }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-path-concat".into(),
        message: "Use `path.join()` or `path.resolve()` instead of string concatenation with `__dirname`/`__filename`.".into(),
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
    fn flags_dirname_plus_string() {
        let d = run_on(r#"const p = __dirname + '/foo';"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_filename_plus_string() {
        assert_eq!(run_on(r#"const p = __filename + '/bar';"#).len(), 1);
    }

    #[test]
    fn flags_string_plus_dirname() {
        assert_eq!(run_on(r#"const p = '/prefix' + __dirname;"#).len(), 1);
    }

    #[test]
    fn allows_path_join() {
        assert!(run_on("const p = path.join(__dirname, 'foo');").is_empty());
    }

    #[test]
    fn allows_normal_concat() {
        assert!(run_on("const p = a + b;").is_empty());
    }
}
