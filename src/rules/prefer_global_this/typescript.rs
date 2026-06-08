//! prefer-global-this — flag `window.X` / `self.X` / `global.X` accesses
//! that should be `globalThis.X`.
//!
//! Detection: walk `member_expression` nodes whose object is the bare
//! identifier `window`, `self`, or `global`. Skip when:
//!   - The property is window-specific (e.g. `innerWidth`, `addEventListener`).
//!   - The expression is the operand of a `typeof` check.
//!   - The project's package.json declares a browser-like target
//!     (browserslist / engines.vscode / engines.electron).

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::ProjectCtx;
use std::path::Path;

/// True if the project's `package.json` declares a browser-like runtime
/// target — VSCode extension (`engines.vscode`), Electron app
/// (`engines.electron`), or a browser build target (`browserslist`). In
/// those environments `window` is the real DOM `Window` object and is
/// NOT interchangeable with `globalThis` (different prototype, different
/// set of properties), so we must stay silent. Pure-Node projects — no
/// manifest, or a manifest without any of these keys — still get the
/// rule applied.
fn project_allows_window(project: &ProjectCtx, path: &Path) -> bool {
    let Some(pkg) = project.nearest_package_json(path) else {
        return false;
    };
    pkg.has_browserslist
        || pkg.engines.contains_key("vscode")
        || pkg.engines.contains_key("electron")
}

/// Window-specific APIs that should remain as `window.X`.
const WINDOW_SPECIFIC: &[&str] = &[
    "close",
    "closed",
    "stop",
    "focus",
    "blur",
    "frames",
    "length",
    "top",
    "opener",
    "parent",
    "frameElement",
    "open",
    "postMessage",
    "navigation",
    "name",
    "locationbar",
    "menubar",
    "personalbar",
    "scrollbars",
    "statusbar",
    "toolbar",
    "status",
    "originAgentCluster",
    // CSSOM View
    "screen",
    "visualViewport",
    "moveTo",
    "moveBy",
    "resizeTo",
    "resizeBy",
    "innerWidth",
    "innerHeight",
    "outerWidth",
    "outerHeight",
    "scrollX",
    "pageXOffset",
    "scrollY",
    "pageYOffset",
    "scroll",
    "scrollTo",
    "scrollBy",
    "screenX",
    "screenLeft",
    "screenY",
    "screenTop",
    "devicePixelRatio",
    // Events
    "addEventListener",
    "removeEventListener",
    "dispatchEvent",
    "onresize",
    "onblur",
    "onfocus",
    "onload",
    "onscroll",
    "onbeforeunload",
    "onmessage",
    "onpagehide",
    "onpageshow",
    "onunload",
];

/// True if `node` is the operand of a `typeof` unary expression.
fn is_under_typeof(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "unary_expression"
            && parent
                .child_by_field_name("operator")
                .and_then(|o| o.utf8_text(source).ok())
                == Some("typeof")
        {
            return true;
        }
        // Stop walking up once we're past the immediate member chain;
        // typeof binds to its argument, which can be a member chain.
        if !matches!(parent.kind(), "member_expression" | "subscript_expression") {
            return false;
        }
        cur = parent;
    }
    false
}

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    let Some(obj) = node.child_by_field_name("object") else { return };
    if obj.kind() != "identifier" {
        return;
    }
    let name = obj.utf8_text(source).unwrap_or("");
    if !matches!(name, "window" | "self" | "global") {
        return;
    }

    // Project allowlist — browser-like targets keep `window` as the real
    // DOM Window object. The check requires a `package.json` lookup, so gate
    // it behind the rare identifier match above rather than paying it on
    // every `a.b` access.
    if project_allows_window(ctx.project, ctx.path) {
        return;
    }

    // Only the innermost `window.X` in a chain like `window.a.b` matches
    // (object = bare identifier `window`). Outer member expressions have
    // `object` set to another member_expression, so they pass the
    // identifier check above and are skipped naturally.
    let Some(prop) = node.child_by_field_name("property") else { return };
    let prop_text = prop.utf8_text(source).unwrap_or("");

    if name == "window" && WINDOW_SPECIFIC.contains(&prop_text) {
        return;
    }

    if is_under_typeof(node, source) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "prefer-global-this",
        format!("Prefer `globalThis` over `{name}`. Replace `{name}.` with `globalThis.`."),
        Severity::Warning,
    ));
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
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::fs;
    use tempfile::TempDir;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    /// Build a temp project with an optional package.json body, then run
    /// the check on a source file placed inside `src/`. Returns the
    /// tempdir handle (so the caller keeps it alive) and the diagnostics.
    fn run_in_project(package_json: Option<&str>, source: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        if let Some(body) = package_json {
            fs::write(dir.path().join("package.json"), body).unwrap();
        }
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let file = src_dir.join("t.ts");
        fs::write(&file, source).unwrap();

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let diags = Check.check(&CheckCtx::for_test(&file, source), &tree);
        (dir, diags)
    }

    #[test]
    fn flags_window_location() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const url = window.location;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_self_in_worker() {
        let d = crate::rules::test_helpers::run_rule(&Check, "self.fetch('/api');", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_global_process() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const env = global.process.env;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn allows_global_this() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const url = globalThis.location;", "t.ts").is_empty());
    }

    #[test]
    fn allows_window_specific_close() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "window.close();", "t.ts").is_empty());
    }

    #[test]
    fn allows_window_specific_inner_width() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const w = window.innerWidth;", "t.ts").is_empty());
    }

    #[test]
    fn ignores_typeof_window() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "if (typeof window.x !== 'undefined') {}", "t.ts").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "// window.location is the URL", "t.ts").is_empty());
    }

    #[test]
    fn skips_when_engines_vscode_set() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "vscode": "^1.85.0" } }"#),
            "const x = window.foo;",
        );
        assert!(
            diags.is_empty(),
            "VSCode extension should not flag window.foo: {diags:?}"
        );
    }

    #[test]
    fn skips_when_engines_electron_set() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "electron": "^28.0.0" } }"#),
            "const x = window.foo;",
        );
        assert!(
            diags.is_empty(),
            "Electron app should not flag window.foo: {diags:?}"
        );
    }

    #[test]
    fn skips_when_browserslist_present() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "browserslist": ["> 0.5%", "last 2 versions"] }"#),
            "const x = window.foo;",
        );
        assert!(
            diags.is_empty(),
            "Browser target should not flag window.foo: {diags:?}"
        );
    }

    #[test]
    fn fires_on_pure_node_project() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": { "express": "^4.0.0" } }"#),
            "const x = window.foo;",
        );
        assert_eq!(
            diags.len(),
            1,
            "Pure Node project should still flag window.foo: {diags:?}"
        );
    }
}
