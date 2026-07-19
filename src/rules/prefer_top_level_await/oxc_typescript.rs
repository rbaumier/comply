//! OXC backend for prefer-top-level-await.
//!
//! Only fires in an ES-module context (a `.mjs`/`.mts` extension, or a nearest
//! `package.json` declaring `"type": "module"`), where top-level `await` is
//! valid. In CommonJS (`.cjs`, or a `.ts`/`.js` file compiled to CJS without
//! `"type": "module"`) the suggested rewrite would be a runtime SyntaxError, so
//! the rule stays silent.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Top-level await is only valid in an ES-module context (a `.mjs`/`.mts`
        // extension or a nearest `package.json` declaring `"type": "module"`). In
        // CommonJS (`.cjs`, or a `.ts`/`.js` file compiled to CJS without `"type":
        // "module"`) the suggested rewrite would be a runtime SyntaxError.
        if !crate::rules::module_system::is_es_module_context_cached(ctx) {
            return;
        }

        // Check if this call is at the top level
        if !is_top_level_call(node, semantic) {
            return;
        }

        // Pattern 1: async IIFE — `(async () => { ... })()`
        if is_async_iife(call) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-top-level-await".into(),
                message: "Prefer top-level await over an async IIFE.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Pattern 2: top-level call to an async function defined at top level
        // Simple case: `main()` where `async function main() { ... }` exists
        let func_name = match &call.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            // Handle `main().then(...)` — the callee is a member expression
            Expression::StaticMemberExpression(member) => {
                if member.property.name.as_str() == "then" {
                    if let Expression::CallExpression(inner_call) = &member.object {
                        if let Expression::Identifier(id) = &inner_call.callee {
                            Some(id.name.as_str())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        let Some(func_name) = func_name else {
            return;
        };

        // Check if there's a top-level async function declaration with this name
        if has_top_level_async_function(func_name, semantic, ctx.source) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "prefer-top-level-await".into(),
                message: format!(
                    "Prefer top-level await over calling async function `{func_name}()`."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn is_top_level_call(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(node.id());
    if parent_id == node.id() {
        return false;
    }
    let parent = nodes.get_node(parent_id);

    match parent.kind() {
        // Direct child of program: `expr;`
        AstKind::ExpressionStatement(_) => {
            let gp_id = nodes.parent_id(parent_id);
            if gp_id == parent_id {
                return true; // root
            }
            matches!(nodes.get_node(gp_id).kind(), AstKind::Program(_))
        }
        AstKind::Program(_) => true,
        _ => false,
    }
}

fn is_async_iife(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::ParenthesizedExpression(paren) => match &paren.expression {
            Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
            Expression::FunctionExpression(func) => func.r#async,
            _ => false,
        },
        _ => false,
    }
}

fn has_top_level_async_function(
    name: &str,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> bool {
    let nodes = semantic.nodes();
    for node in nodes.iter() {
        let AstKind::Function(func) = node.kind() else {
            continue;
        };
        if !func.r#async {
            continue;
        }
        let Some(ref id) = func.id else {
            continue;
        };
        if id.name.as_str() != name {
            continue;
        }
        // Must be at program level (parent is program or export_statement)
        let parent_id = nodes.parent_id(node.id());
        if parent_id == node.id() {
            continue;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Program(_) | AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
                return true;
            }
            _ => {}
        }
    }
    false
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, path)
    }

    // Regression for #4421: the canonical NestJS entrypoint at a `.ts` path with
    // no ES-module context is CommonJS — top-level `await` would be a runtime
    // SyntaxError there, so the rule must stay silent.
    #[test]
    fn does_not_flag_async_fn_call_in_commonjs_ts() {
        let src = "async function bootstrap() { await x(); }\nbootstrap();";
        assert!(
            run_at(src, "examples/nest/src/main.ts").is_empty(),
            "top-level await is invalid in a CommonJS `.ts` file"
        );
    }

    // An async IIFE at a `.ts` path (no ESM context) is also CommonJS and must
    // not be flagged.
    #[test]
    fn does_not_flag_async_iife_in_commonjs_ts() {
        let src = "(async () => { await x(); })();";
        assert!(run_at(src, "src/main.ts").is_empty());
    }

    // The same async-fn-call pattern at a `.mts` path is ESM by extension, so
    // top-level await is valid and the rule still fires.
    #[test]
    fn flags_async_fn_call_in_esm_mts() {
        let src = "async function bootstrap() { await x(); }\nbootstrap();";
        let d = run_at(src, "src/main.mts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bootstrap()"));
    }

    // An async IIFE at a `.mjs` path is ESM by extension and is still flagged.
    #[test]
    fn flags_async_iife_in_esm_mjs() {
        let src = "(async () => { await x(); })();";
        let d = run_at(src, "src/main.mjs");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("async IIFE"));
    }
}
