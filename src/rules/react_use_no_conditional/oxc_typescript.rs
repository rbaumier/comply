//! OXC backend for react-use-no-conditional — flag `use(...)` inside conditionals/loops.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_semantic::NodeId;
use std::sync::Arc;

fn is_inside_conditional(semantic: &oxc_semantic::Semantic, start_id: NodeId) -> bool {
    let nodes = semantic.nodes();
    let mut cur_id = nodes.parent_id(start_id);
    loop {
        if cur_id == start_id || cur_id == nodes.parent_id(cur_id) {
            return false; // hit root
        }
        let n = nodes.get_node(cur_id);
        match n.kind() {
            // Function-like boundary — stop walking
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            // Conditional/loop constructs
            AstKind::IfStatement(_)
            | AstKind::ConditionalExpression(_)
            | AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::SwitchStatement(_) => return true,
            // && / || / ?? short-circuiting is conditional too
            AstKind::LogicalExpression(_) => return true,
            _ => {}
        }
        let next = nodes.parent_id(cur_id);
        if next == cur_id {
            return false;
        }
        cur_id = next;
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check callee is identifier `use`
        let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "use" {
            return;
        }

        // React 19's `use` is always `import { use } from "react"`. Skip a `use`
        // bound to anything else (another library's hook, a local function) so we
        // only enforce React's conditional-call rule on React's `use`.
        if !crate::oxc_helpers::is_imported_from_react("use", semantic) {
            return;
        }

        if !is_inside_conditional(semantic, node.id()) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`use(...)` is a hook \u{2014} it cannot be called conditionally or inside a loop. \
                      Lift the call to the top of the component."
                .into(),
            severity: Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_use_in_if() {
        let src = "import { use } from 'react';\nfunction C({p, x}: any) { if (x) { const v = use(p); return v; } return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_in_ternary() {
        let src = "import { use } from 'react';\nfunction C({p, x}: any) { const v = x ? use(p) : null; return v; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_in_loop() {
        let src = "import { use } from 'react';\nfunction C({ps}: any) { for (const p of ps) { use(p); } return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_use_imported_from_react_dom() {
        let src = "import { use } from 'react-dom';\nfunction C({p, x}: any) { if (x) { const v = use(p); return v; } return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_at_top_level() {
        let src = "import { use } from 'react';\nfunction C({p}: any) { const v = use(p); return v; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_inside_jsx_attr() {
        let src = "import { use } from 'react';\nfunction C({p}: any) { const v = use(p); return <div>{v}</div>; }";
        assert!(run(src).is_empty());
    }

    // Hono's `use` is imported from its own hooks module, not React, and is
    // intentionally conditional. Without the react-import gate this flags (FP).
    #[test]
    fn allows_use_imported_from_other_framework_in_if() {
        let src = "import { use } from '../../hooks';\nfunction f(x: any) { if (x) { use(p); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_locally_defined_use_in_if() {
        let src = "function use(p: any) {}\nfunction f(x: any) { if (x) { use(p); } }";
        assert!(run(src).is_empty());
    }
}
