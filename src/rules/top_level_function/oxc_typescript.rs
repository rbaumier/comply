//! top-level-function OxcCheck backend — flag top-level
//! `const foo = () => {...}` arrow functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else { return };

        // Must be at top level: parent is Program, or parent is
        // ExportNamedDeclaration whose parent is Program.
        let parent = semantic.nodes().parent_node(node.id());
        let is_top_level = match parent.kind() {
            AstKind::Program(_) => true,
            AstKind::ExportNamedDeclaration(_) => {
                let gp = semantic.nodes().parent_node(parent.id());
                matches!(gp.kind(), AstKind::Program(_))
            }
            _ => false,
        };
        if !is_top_level {
            return;
        }

        let complexity_threshold = ctx.config.threshold("cyclomatic-complexity", "max", ctx.lang);

        for declarator in &decl.declarations {
            let Some(init) = &declarator.init else { continue };
            let oxc_ast::ast::Expression::ArrowFunctionExpression(arrow) = init else {
                continue;
            };

            // An arrow complex enough to exceed the cyclomatic-complexity
            // threshold is already a complexity concern; nudging it to a
            // declaration is noise, and the conversion would additionally
            // expose it to `cognitive-complexity`. Leave it alone. (#596)
            if arrow_cyclomatic_complexity(arrow.span, semantic) > complexity_threshold {
                continue;
            }

            let name = match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.as_str(),
                _ => "<unknown>",
            };

            // PascalCase in a JSX/TSX file = React component convention; the
            // arrow is intentional (avoids hoisting that breaks hook ordering).
            if ctx.lang == Language::Tsx && name.starts_with(|c: char| c.is_uppercase()) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Top-level `const {name} = () => ...` — prefer `function {name}(...) {{ ... }}` \
                     for a named binding, hoisting, and better stack traces."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Cyclomatic complexity of the arrow whose body spans `arrow_span`, counting
/// branching nodes attributed directly to it (not to nested functions). Mirrors
/// the `cyclomatic-complexity` rule so the two stay in agreement.
fn arrow_cyclomatic_complexity(
    arrow_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic<'_>,
) -> usize {
    use oxc_ast::ast::LogicalOperator;
    let nodes = semantic.nodes();
    let mut complexity = 1usize;
    for snode in nodes.iter() {
        let kind = snode.kind();
        let child_span = match kind {
            AstKind::IfStatement(s) => s.span,
            AstKind::ForStatement(s) => s.span,
            AstKind::ForInStatement(s) => s.span,
            AstKind::ForOfStatement(s) => s.span,
            AstKind::WhileStatement(s) => s.span,
            AstKind::DoWhileStatement(s) => s.span,
            AstKind::CatchClause(s) => s.span,
            AstKind::SwitchStatement(s) => s.span,
            AstKind::ConditionalExpression(s) => s.span,
            AstKind::LogicalExpression(s) => s.span,
            _ => continue,
        };
        if child_span.start < arrow_span.start || child_span.end > arrow_span.end {
            continue;
        }
        if let AstKind::LogicalExpression(log) = kind
            && !matches!(
                log.operator,
                LogicalOperator::And | LogicalOperator::Or | LogicalOperator::Coalesce
            )
        {
            continue;
        }
        if nearest_function_span(snode.id(), nodes) != Some(arrow_span) {
            continue;
        }
        complexity += 1;
    }
    complexity
}

/// Walk up ancestors to find the nearest enclosing function's span.
fn nearest_function_span(
    node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> Option<oxc_span::Span> {
    for kind in nodes.ancestor_kinds(node_id).skip(1) {
        match kind {
            AstKind::Function(f) => return Some(f.span),
            AstKind::ArrowFunctionExpression(a) => return Some(a.span),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{run_oxc_ts, run_oxc_tsx, run_oxc_tsx_with_path};

    #[test]
    fn flags_simple_top_level_arrow() {
        assert_eq!(run_oxc_ts("const greet = (n: string) => `hi ${n}`;", &Check).len(), 1);
    }

    #[test]
    fn no_fp_high_complexity_arrow_issue_596() {
        // A module-scope dispatch arrow whose cyclomatic complexity exceeds the
        // cyclomatic-complexity threshold (15): converting it to a declaration
        // would only trade this nudge for a worse cognitive-complexity hit.
        let mut body = String::from("const dispatch = (code: string) => {\n");
        for c in 'a'..='t' {
            body.push_str(&format!("  if (code === '{c}') return '{c}';\n"));
        }
        body.push_str("  return 'default';\n};\n");
        let diags = run_oxc_ts(&body, &Check);
        assert!(diags.is_empty(), "high-complexity arrow should not flag, got {diags:#?}");
    }

    // React component exemption in .tsx/.jsx files (issue #792)

    #[test]
    fn no_fp_react_component_no_props_tsx() {
        let diags = run_oxc_tsx("const App = () => 42;", &Check);
        assert!(diags.is_empty(), "PascalCase arrow in .tsx should not flag (React component), got {diags:#?}");
    }

    #[test]
    fn no_fp_react_component_with_props_tsx() {
        let diags = run_oxc_tsx("const MyComponent = (props: Props) => props.name;", &Check);
        assert!(diags.is_empty(), "PascalCase arrow with props in .tsx should not flag, got {diags:#?}");
    }

    #[test]
    fn still_flags_camelcase_arrow_in_tsx() {
        let diags = run_oxc_tsx("const parseData = (input: string) => input;", &Check);
        assert_eq!(diags.len(), 1, "camelCase utility arrow in .tsx should still flag");
    }

    #[test]
    fn still_flags_pascalcase_arrow_in_ts() {
        let diags = run_oxc_ts("const MyHelper = (x: number) => x + 1;", &Check);
        assert_eq!(diags.len(), 1, "PascalCase arrow in .ts should still flag (no JSX)");
    }

    #[test]
    fn no_fp_react_component_in_jsx() {
        let diags = run_oxc_tsx_with_path("const Button = () => 42;", &Check, "t.jsx");
        assert!(diags.is_empty(), "PascalCase arrow in .jsx should not flag, got {diags:#?}");
    }



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_top_level_const_arrow() {
        let diags = run_on("const foo = () => 42;");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "top-level-function");
        assert!(diags[0].message.contains("foo"));
    }


    #[test]
    fn flags_top_level_let_arrow() {
        let diags = run_on("let foo = () => 42;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_function_declaration() {
        assert!(run_on("function foo() { return 42; }").is_empty());
    }


    #[test]
    fn allows_nested_arrow() {
        // Inside a function body — not top-level.
        let src = "function outer() { const inner = () => 1; return inner; }";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_non_function_top_level_const() {
        assert!(run_on("const x = 42;").is_empty());
    }


    #[test]
    fn allows_arrow_as_callback() {
        let src = "[1, 2, 3].map(x => x * 2);";
        assert!(run_on(src).is_empty());
    }
}
