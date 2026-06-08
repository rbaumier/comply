//! react-no-find-dom-node AST backend.
//!
//! Flags `call_expression` nodes whose callee is either:
//! - a `member_expression` ending in `.findDOMNode` (e.g. `ReactDOM.findDOMNode(...)`)
//! - the bare identifier `findDOMNode` (e.g. `findDOMNode(ref.current)`)

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };

    let matched = match callee.kind() {
        "member_expression" => {
            let Some(prop) = callee.child_by_field_name("property") else { return };
            let Ok(prop_name) = prop.utf8_text(source) else { return };
            prop_name == "findDOMNode"
        }
        "identifier" => {
            let Ok(name) = callee.utf8_text(source) else { return };
            name == "findDOMNode"
        }
        _ => false,
    };

    if !matched {
        return;
    }

    let pos = callee.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`findDOMNode` is deprecated in React 19 — use refs instead.".into(),
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

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_react_dom_find_dom_node() {
        let src = "const n = ReactDOM.findDOMNode(this);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_bare_find_dom_node() {
        let src = "import { findDOMNode } from 'react-dom'; const n = findDOMNode(ref.current);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_ref_usage() {
        let src = "const ref = useRef(null); const n = ref.current;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_member_call() {
        let src = "ReactDOM.render(<App />, root);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unrelated_identifier_call() {
        let src = "const n = findNode(ref);";
        assert!(run_on(src).is_empty());
    }
}
