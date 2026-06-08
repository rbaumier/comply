//! no-undefined-argument backend — flag `undefined` passed as a function argument.

use crate::diagnostic::{Diagnostic, Severity};

fn is_create_context_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "call_expression" {
            if let Some(func) = parent.child_by_field_name("function") {
                let text = func.utf8_text(source).unwrap_or("");
                return text == "createContext" || text.ends_with(".createContext");
            }
        }
    }
    false
}

fn is_in_assertion_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "call_expression" {
            if let Some(func) = n.child_by_field_name("function") {
                let text = func.utf8_text(source).unwrap_or("");
                if text.contains("expect") || text.contains("assert") {
                    return true;
                }
            }
        }
        cur = n.parent();
    }
    false
}

crate::ast_check! { on ["arguments"] prefilter = ["undefined"] => |node, source, ctx, diagnostics|
    if is_in_assertion_chain(node, source) { return; }
    if is_create_context_call(node, source) { return; }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "undefined" {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-undefined-argument".into(),
                message: "Do not pass `undefined` as an argument \u{2014} omit the argument instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    #[test]
    fn flags_sole_undefined_arg() {
        let d = crate::rules::test_helpers::run_rule(&Check, "foo(undefined);", "t.ts");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-undefined-argument");
    }

    #[test]
    fn flags_undefined_among_args() {
        let d = crate::rules::test_helpers::run_rule(&Check, "foo(x, undefined, y);", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_no_undefined() {
        let d = crate::rules::test_helpers::run_rule(&Check, "foo(x, y);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_undefined_in_variable_name() {
        let d = crate::rules::test_helpers::run_rule(&Check, "foo(undefinedValue);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_undefined_in_expect_matcher() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(spy).toHaveBeenCalledWith(state, undefined);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_undefined_in_to_equal() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(result).toEqual(undefined);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn still_flags_outside_expect() {
        let d = crate::rules::test_helpers::run_rule(&Check, "doStuff(undefined);", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_react_create_context_undefined() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const Ctx = React.createContext<Foo | undefined>(undefined);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_bare_create_context_undefined() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const Ctx = createContext<Foo | undefined>(undefined);", "t.ts");
        assert!(d.is_empty());
    }
}
