//! react-use-state-lazy-init — oxc backend for TSX.
//!
//! Flags `useState(arg)` only when `arg` is a genuinely expensive initializer:
//! a call expression (`compute()`, `JSON.parse(s)`, `obj.compute()`), a
//! `new X(...)` construction, or a member access rooted on a browser global
//! (`window.innerWidth`, `document.title`). Plain property access on a local
//! value (`todo.is_complete`, `props.value`, `this.x`) is a single memory read
//! and is left untouched.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, receiver_root_identifier};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Browser globals whose member access does real work or crashes in SSR, so
/// reading them in a bare `useState(...)` warrants the lazy form. Property
/// access on any other root (a local, a prop, `this`) is cheap.
const BROWSER_GLOBALS: &[&str] = &[
    "window",
    "document",
    "navigator",
    "screen",
    "location",
    "performance",
    "localStorage",
    "sessionStorage",
    "history",
];

/// Whether `expr` is an initializer expensive enough to justify the lazy
/// `useState(() => …)` form. Calls and constructions always qualify; member
/// access qualifies only when rooted on a browser global.
fn is_expensive_init(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(_) | Expression::NewExpression(_) => true,
        Expression::StaticMemberExpression(_)
        | Expression::ComputedMemberExpression(_)
        | Expression::PrivateFieldExpression(_) => receiver_root_identifier(expr)
            .is_some_and(|root| BROWSER_GLOBALS.contains(&root.as_str())),
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        // Fire only when the callee resolves to React's `useState` — a named
        // import from `react`/`react-dom`. A same-named binding from another
        // module (e.g. a Vue composable wrapping `ref()`) or a local declaration
        // is not React's render-time hook, so the every-render/SSR rationale does
        // not apply.
        let is_react_use_state = match &call.callee {
            Expression::Identifier(id) => {
                id.name == "useState"
                    && crate::oxc_helpers::is_imported_from_react("useState", semantic)
            }
            _ => false,
        };
        if !is_react_use_state {
            return;
        }
        // Flag only when the first argument is an expensive initializer.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let expr = first_arg.as_expression();
        let Some(expr) = expr else { return };
        if !is_expensive_init(expr.without_parentheses()) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-use-state-lazy-init".into(),
            message: "`useState(expensive())` runs the initializer on every render \
                      and crashes in SSR. Wrap in a lazy function: \
                      `useState(() => expensive())`.".into(),
            severity: Severity::Warning,
            span: None,
        });
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    const REACT_IMPORT: &str = "import { useState } from 'react';\n";

    #[test]
    fn flags_use_state_with_function_call() {
        let src = format!("{REACT_IMPORT}const [w] = useState(getInitial());");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn flags_use_state_with_browser_api() {
        let src = format!("{REACT_IMPORT}const [w] = useState(window.innerWidth);");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn allows_lazy_init() {
        let src = format!("{REACT_IMPORT}const [w] = useState(() => getInitial());");
        assert!(run_on(&src).is_empty());
    }

    #[test]
    fn allows_primitive_init() {
        let src = format!("{REACT_IMPORT}const [w] = useState(0);");
        assert!(run_on(&src).is_empty());
    }

    // Regression #2131: cheap property access is not an expensive initializer.
    #[test]
    fn allows_prop_access_on_local() {
        assert!(run_on(&format!("{REACT_IMPORT}const [c, sc] = useState(props.initialCount);")).is_empty());
        assert!(run_on(&format!("{REACT_IMPORT}const [c, sc] = useState(obj.a.b);")).is_empty());
        assert!(run_on(&format!("{REACT_IMPORT}const [c, sc] = useState(this.x);")).is_empty());
        assert!(run_on(&format!("{REACT_IMPORT}const [c, sc] = useState(arr[0]);")).is_empty());
    }

    // A call on a member (`obj.compute()`) is a call expression, still expensive.
    #[test]
    fn flags_method_call_on_member() {
        let src = format!("{REACT_IMPORT}const [c, sc] = useState(obj.compute());");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn flags_new_expression() {
        let src = format!("{REACT_IMPORT}const [c, sc] = useState(new Map());");
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn flags_json_parse() {
        let src = format!("{REACT_IMPORT}const [c, sc] = useState(JSON.parse(s));");
        assert_eq!(run_on(&src).len(), 1);
    }

    // Regression #7297: `useState` default-imported from a non-react module (a
    // local Vue composable wrapping `ref()`) is not React's hook — its
    // initializer runs once, so an expensive arg must not be flagged.
    #[test]
    fn skips_non_react_default_import_use_state() {
        let src = "import useState from '../../../_util/hooks/useState';\n\
                   const [s, setS] = useState(collectFilterStates(cols, true));";
        assert!(run_on(src).is_empty());
    }

    // React's `useState` (named import) with a non-lazy expensive initializer
    // stays flagged.
    #[test]
    fn flags_react_named_import_expensive_init() {
        let src = "import { useState } from 'react';\n\
                   const [s, setS] = useState(expensive());";
        assert_eq!(run_on(src).len(), 1);
    }

    // A locally declared `useState` is not React's and does not fire.
    #[test]
    fn skips_local_use_state() {
        let src = "function useState() {}\n\
                   const [s, setS] = useState(expensive());";
        assert!(run_on(src).is_empty());
    }
}
