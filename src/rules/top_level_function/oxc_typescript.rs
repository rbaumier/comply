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

            // An explicit type annotation on the binding (`const fn: T = () => …`)
            // cannot be carried by a function declaration — there is no
            // `function fn: T(...)` syntax — so converting would drop the type.
            // Leave the arrow form alone.
            if declarator.type_annotation.is_some() {
                continue;
            }

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
    

    #[test]
    fn flags_simple_top_level_arrow() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const greet = (n: string) => `hi ${n}`;", "t.ts").len(), 1);
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
        let diags = crate::rules::test_helpers::run_rule(&Check, &body, "t.ts");
        assert!(diags.is_empty(), "high-complexity arrow should not flag, got {diags:#?}");
    }

    // React component exemption in .tsx/.jsx files (issue #792)

    #[test]
    fn no_fp_react_component_no_props_tsx() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "const App = () => 42;", "t.tsx");
        assert!(diags.is_empty(), "PascalCase arrow in .tsx should not flag (React component), got {diags:#?}");
    }

    #[test]
    fn no_fp_react_component_with_props_tsx() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "const MyComponent = (props: Props) => props.name;", "t.tsx");
        assert!(diags.is_empty(), "PascalCase arrow with props in .tsx should not flag, got {diags:#?}");
    }

    #[test]
    fn still_flags_camelcase_arrow_in_tsx() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "const parseData = (input: string) => input;", "t.tsx");
        assert_eq!(diags.len(), 1, "camelCase utility arrow in .tsx should still flag");
    }

    #[test]
    fn still_flags_pascalcase_arrow_in_ts() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "const MyHelper = (x: number) => x + 1;", "t.ts");
        assert_eq!(diags.len(), 1, "PascalCase arrow in .ts should still flag (no JSX)");
    }

    #[test]
    fn no_fp_react_component_in_jsx() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "const Button = () => 42;", "t.jsx");
        assert!(diags.is_empty(), "PascalCase arrow in .jsx should not flag, got {diags:#?}");
    }

    // Typed arrow exemption (issue #1911): a `const fn: T = () => {}` cannot be
    // rewritten as a function declaration without losing the type annotation.

    #[test]
    fn no_fp_typed_arrow_issue_1911() {
        let src = "const ordinalNumber: LocalizeFn<number> = (dirtyNumber, _options) => {\n  const number = Number(dirtyNumber);\n  return number + \".\";\n};";
        let diags = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(diags.is_empty(), "typed top-level arrow should not flag, got {diags:#?}");
    }

    #[test]
    fn still_flags_untyped_arrow() {
        let diags =
            crate::rules::test_helpers::run_rule(&Check, "const ordinalNumber = (n: number) => n + 1;", "t.ts");
        assert_eq!(diags.len(), 1, "untyped top-level arrow should still flag");
    }
}
