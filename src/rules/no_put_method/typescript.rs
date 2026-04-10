//! no-put-method backend — flag `method: 'PUT'` in fetch/request calls.
//!
//! Why: PUT means "replace the entire resource". Almost every partial
//! update is wrongly shipped as PUT when the author wanted PATCH. If you
//! genuinely need full replacement, you probably want a specialized
//! endpoint that takes every field explicitly. When in doubt, PATCH.
//!
//! Detection: walk `pair` nodes (object literal key:value entries) whose
//! key is `method` and value is the string literal `'PUT'` or `"PUT"`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((key, value)) = crate::rules::object_literal::object_pair(node, source) else {
        return;
    };
    if key != "method" {
        return;
    }
    let value_norm = value.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if value_norm != "PUT" {
        return;
    }
    let pos = node.child_by_field_name("value").unwrap().start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-put-method".into(),
        message: "`method: 'PUT'` — PUT replaces the entire resource. Most \
                  update-style endpoints want PATCH (partial update). If you \
                  genuinely need full replacement, add a comment explaining why."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_put_method() {
        assert_eq!(
            run_on("fetch(url, { method: 'PUT', body });").len(),
            1
        );
    }

    #[test]
    fn flags_put_method_double_quoted() {
        assert_eq!(
            run_on("fetch(url, { method: \"PUT\" });").len(),
            1
        );
    }

    #[test]
    fn allows_patch_method() {
        assert!(run_on("fetch(url, { method: 'PATCH' });").is_empty());
    }

    #[test]
    fn allows_post_get_delete() {
        for method in ["POST", "GET", "DELETE", "PATCH"] {
            let source = format!("fetch(url, {{ method: '{method}' }});");
            assert!(run_on(&source).is_empty(), "{method} should be allowed");
        }
    }
}
