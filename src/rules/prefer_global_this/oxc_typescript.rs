//! prefer-global-this OXC backend — flag `window.X` / `self.X` / `global.X`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
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

    // The rule only flags `window.`/`self.`/`global.` member access, so a file
    // carrying none of these identifiers can never trigger it.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["window", "self", "global"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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

        // The project allowlist requires a `package.json` lookup (locked,
        // per-directory memoised) — gate it behind the rare identifier match
        // above so the vast majority of `a.b` accesses skip it entirely.
        if project_allows_window(ctx.project, ctx.path) {
            return;
        }

        let prop_text = member.property.name.as_str();

        if name == "window" && WINDOW_SPECIFIC.contains(&prop_text) {
            return;
        }

        if is_under_typeof(node, semantic) {
            return;
        }

        // Inside a Playwright/Puppeteer `*.evaluate(...)` callback the code runs
        // in the browser page realm, where `window` is the intended global.
        if crate::oxc_helpers::is_in_browser_eval_callback(node, semantic) {
            return;
        }

        // A file that feature-detects this global with a `typeof` check
        // (`typeof window !== "undefined"`) is deliberately environment-aware
        // code where the bare alias is the intended object, not a portability
        // oversight — e.g. a browser-only library guarding `window.matchMedia`.
        if crate::oxc_helpers::file_typeof_guards(ctx.source, semantic).guards(name) {
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
