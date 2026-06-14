//! node-global-require oxc backend — require() must be at module top level.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Image, font, and media extensions that the Metro/Expo bundler resolves
/// statically when passed to `require()` (the documented React Native pattern).
const STATIC_ASSET_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".ttf", ".otf", ".woff", ".woff2",
    ".mp4", ".webm", ".mov", ".m4v", ".mp3", ".wav", ".aac", ".m4a",
];

fn is_static_asset_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    STATIC_ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Test-runner lifecycle hooks whose callback bodies legitimately call
/// `require()`: after `jest.resetModules()` / `vi.resetModules()` the module
/// registry is cleared, and a fresh CommonJS `require()` is the only way to
/// re-import the reset module (a static `import` is hoisted and cannot observe
/// the reset). The require lives inside the hook callback by necessity.
const LIFECYCLE_HOOK_IDENTS: &[&str] = &["beforeEach", "beforeAll", "afterEach", "afterAll"];

/// Identifier name of a hook call's callee for the bare (`beforeEach(...)`) and
/// member (`test.beforeEach(...)`) forms; `None` for any other callee shape.
fn hook_callee_name<'a>(call: &'a oxc_ast::ast::CallExpression) -> Option<&'a str> {
    match &call.callee {
        oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
        oxc_ast::ast::Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        _ => None,
    }
}

/// True when `func_node` is the callback argument of a test lifecycle hook call
/// (`beforeEach`/`beforeAll`/`afterEach`/`afterAll`). The callback's immediate
/// parent in oxc's semantic tree is the `CallExpression` itself (arguments have
/// no wrapper node); requiring the function to appear in `arguments` excludes an
/// IIFE in the callee position.
fn is_lifecycle_hook_callback(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(func_node.id());
    let AstKind::CallExpression(call) = parent.kind() else {
        return false;
    };
    let Some(name) = hook_callee_name(call) else {
        return false;
    };
    if !LIFECYCLE_HOOK_IDENTS.contains(&name) {
        return false;
    }
    let span = func_node.kind().span();
    call.arguments.iter().any(|arg| arg.span() == span)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "require" {
            return;
        }

        // React Native / Metro bundle static assets via `require("./img.png")`
        // inside JSX — these are bundler-managed asset references, not CommonJS
        // module loads, and the documented pattern requires them inline. Exempt
        // string-literal arguments pointing at a known static-asset extension.
        if let Some(oxc_ast::ast::Argument::StringLiteral(lit)) = call.arguments.first()
            && is_static_asset_path(lit.value.as_str())
        {
            return;
        }

        // Walk ancestors: require is OK if all ancestors are top-level.
        let mut in_function = false;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    // A `require()` inside a test lifecycle-hook callback
                    // (`beforeEach`/`beforeAll`/`afterEach`/`afterAll`) is the
                    // documented Jest/Vitest way to re-import a module after
                    // `resetModules()`; do not flag it.
                    if is_lifecycle_hook_callback(ancestor, semantic) {
                        return;
                    }
                    in_function = true;
                    break;
                }
                AstKind::MethodDefinition(_)
                | AstKind::IfStatement(_)
                | AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::TryStatement(_)
                | AstKind::SwitchStatement(_) => {
                    in_function = true;
                    break;
                }
                _ => {}
            }
        }

        if !in_function {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected `require()`. Move it to the top-level module scope.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_image_asset_require_in_jsx() {
        let d = run(
            r#"const x = <Image source={require("@/assets/images/partial-react-logo.png")} />;"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_font_asset_require() {
        assert!(run(r#"function f() { return require("./assets/fonts/Inter.ttf"); }"#).is_empty());
    }

    #[test]
    fn flags_module_require_in_function() {
        let d = run(r#"function init() { const fs = require("fs"); return fs; }"#);
        assert_eq!(d.len(), 1);
    }

    // Regression for #1727: `require()` after `jest.resetModules()` inside a
    // `beforeEach` hook is the only way to re-import a reset module.
    #[test]
    fn allows_require_in_before_each_hook() {
        let d = run(
            r#"beforeEach(() => { jest.resetModules(); const m = require("../act-compat").default; });"#,
        );
        assert!(d.is_empty());
    }

    // Regression for #1727: same pattern with `beforeAll`.
    #[test]
    fn allows_require_in_before_all_hook() {
        let d = run(
            r#"beforeAll(() => { process.env.X = "true"; const rtl = require("../"); });"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_require_in_after_each_hook() {
        let d = run(r#"afterEach(() => { const m = require("./reset"); });"#);
        assert!(d.is_empty());
    }

    // Vitest member form `test.beforeEach(() => ...)`.
    #[test]
    fn allows_require_in_member_form_hook() {
        let d = run(r#"test.beforeEach(() => { const m = require("./reset"); });"#);
        assert!(d.is_empty());
    }

    // Negative space: a genuine production `require()` inside a non-hook callback
    // is still flagged — the exemption is scoped to the four lifecycle hooks.
    #[test]
    fn flags_require_in_non_hook_callback() {
        let d = run(r#"setup(() => { const fs = require("fs"); return fs; });"#);
        assert_eq!(d.len(), 1);
    }
}
