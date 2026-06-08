//! test-check-exception backend — walk `call_expression` nodes looking for
//! `.toThrow()` with no arguments in test files.
//!
//! Detection: find `member_expression` calls where the property is `toThrow`
//! and the arguments list is empty.

use crate::diagnostic::{Diagnostic, Severity};

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

crate::ast_check! { on ["call_expression"] prefilter = ["toThrow"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    // Check the function part is a member_expression with property "toThrow"
    let Some(func) = node.child_by_field_name("function") else {
        return;
    };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return;
    };
    let prop_text = &source[prop.byte_range()];
    if prop_text != b"toThrow" {
        return;
    }
    // Skip `.not.toThrow()` — asserts no error is thrown; no argument needed or meaningful
    if let Some(obj) = func.child_by_field_name("object") {
        if obj.kind() == "member_expression" {
            if let Some(obj_prop) = obj.child_by_field_name("property") {
                if &source[obj_prop.byte_range()] == b"not" {
                    return;
                }
            }
        }
    }
    // Check arguments are empty
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    // arguments node includes parens; named children are the actual args
    let mut cursor = args.walk();
    let arg_count = args.named_children(&mut cursor).count();
    if arg_count > 0 {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "test-check-exception".into(),
        message: "`.toThrow()` without specifying error type or message — any error will pass.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
        // Use a test-file path so the check fires
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx =
            crate::rules::backend::CheckCtx::for_test(std::path::Path::new("foo.test.ts"), source);
        <Check as crate::rules::backend::AstCheck>::check(&Check, &ctx, &tree)
    }

    #[test]
    fn flags_empty_to_throw() {
        let d = run_on("expect(() => doThing()).toThrow();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_throw_with_error_type() {
        assert!(run_on("expect(() => doThing()).toThrow(TypeError);").is_empty());
    }

    #[test]
    fn allows_to_throw_with_message() {
        assert!(run_on(r#"expect(() => doThing()).toThrow("bad input");"#).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        // Use run_ts which defaults to "t.ts" (not a test file)
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(() => doThing()).toThrow();", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn no_fp_on_not_to_throw() {
        // .not.toThrow() asserts no error is thrown — no argument needed (Closes #440)
        assert!(run_on("expect(() => fn()).not.toThrow();").is_empty());
    }
}
