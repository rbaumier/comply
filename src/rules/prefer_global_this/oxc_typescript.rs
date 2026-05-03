//! prefer-global-this OXC backend — flag `window.X` / `self.X` / `global.X`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if the project's `package.json` declares a browser-like runtime target.
fn project_allows_window(
    project: &crate::project::ProjectCtx,
    path: &std::path::Path,
) -> bool {
    let Some(pkg) = project.nearest_package_json(path) else {
        return false;
    };
    pkg.has_browserslist
        || pkg.engines.contains_key("vscode")
        || pkg.engines.contains_key("electron")
}

/// Window-specific APIs that should remain as `window.X`.
const WINDOW_SPECIFIC: &[&str] = &[
    "close", "closed", "stop", "focus", "blur", "frames", "length", "top",
    "opener", "parent", "frameElement", "open", "postMessage", "navigation",
    "name", "locationbar", "menubar", "personalbar", "scrollbars", "statusbar",
    "toolbar", "status", "originAgentCluster",
    "screen", "visualViewport", "moveTo", "moveBy", "resizeTo", "resizeBy",
    "innerWidth", "innerHeight", "outerWidth", "outerHeight",
    "scrollX", "pageXOffset", "scrollY", "pageYOffset", "scroll", "scrollTo",
    "scrollBy", "screenX", "screenLeft", "screenY", "screenTop",
    "devicePixelRatio",
    "addEventListener", "removeEventListener", "dispatchEvent",
    "onresize", "onblur", "onfocus", "onload", "onscroll",
    "onbeforeunload", "onmessage", "onpagehide", "onpageshow", "onunload",
];

/// True if `node` is the operand of a `typeof` unary expression.
fn is_under_typeof<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::UnaryExpression(unary) => {
                if unary.operator == UnaryOperator::Typeof {
                    return true;
                }
            }
            // Stop walking up once past member chain.
            AstKind::StaticMemberExpression(_)
            | AstKind::ComputedMemberExpression(_) => continue,
            _ => return false,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if project_allows_window(ctx.project, ctx.path) {
            return;
        }

        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        // Object must be a bare identifier.
        let Expression::Identifier(obj) = &member.object else {
            return;
        };

        let name = obj.name.as_str();
        if !matches!(name, "window" | "self" | "global") {
            return;
        }

        let prop_text = member.property.name.as_str();

        if name == "window" && WINDOW_SPECIFIC.contains(&prop_text) {
            return;
        }

        if is_under_typeof(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "prefer-global-this".into(),
            message: format!("Prefer `globalThis` over `{name}`. Replace `{name}.` with `globalThis.`."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
