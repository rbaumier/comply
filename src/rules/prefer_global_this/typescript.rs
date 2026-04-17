use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};

/// Walk up from `file` looking for the nearest `package.json`.
fn find_package_json(file: &Path) -> Option<std::path::PathBuf> {
    let mut current = file.parent();
    while let Some(dir) = current {
        let candidate = dir.join("package.json");
        if candidate.is_file() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

/// True if the project's `package.json` declares a browser-like runtime
/// target — VSCode extension (`engines.vscode`), Electron app
/// (`engines.electron`), or a browser build target (`browserslist`). In
/// those environments `window` is the real DOM `Window` object and is
/// NOT interchangeable with `globalThis` (different prototype, different
/// set of properties), so we must stay silent. Pure-Node projects — no
/// manifest, or a manifest without any of these keys — still get the
/// rule applied.
fn project_allows_window(file: &Path) -> bool {
    let Some(manifest) = find_package_json(file) else {
        return false;
    };
    let Ok(raw) = std::fs::read_to_string(&manifest) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    if value.get("browserslist").is_some() {
        return true;
    }
    if let Some(engines) = value.get("engines").and_then(|v| v.as_object())
        && (engines.contains_key("vscode") || engines.contains_key("electron"))
    {
        return true;
    }
    false
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

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Check if a `window.X` usage is window-specific (should NOT be flagged).
fn is_window_specific_api(line: &str, dot_pos: usize) -> bool {
    let after = &line[dot_pos + 1..];
    let prop: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    WINDOW_SPECIFIC.iter().any(|&api| api == prop)
}

/// Find usages of `window.`, `self.`, or `global.` that should be `globalThis.`.
fn find_global_this_violations(line: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let globals = &["window", "self", "global"];

    for &name in globals {
        let pattern = format!("{name}.");
        let bytes = line.as_bytes();
        let mut start = 0;

        while start + pattern.len() <= bytes.len() {
            let Some(rel) = line[start..].find(&pattern) else {
                break;
            };
            let abs = start + rel;

            let preceded_by_ident = abs > 0 && is_ident_char(bytes[abs - 1]);
            let dot_pos = abs + name.len();

            let followed_by_prop = dot_pos + 1 < bytes.len() && is_ident_char(bytes[dot_pos + 1]);

            if !preceded_by_ident && followed_by_prop {
                if name == "window" && is_window_specific_api(line, dot_pos) {
                    start = abs + pattern.len();
                    continue;
                }

                let before = line[..abs].trim_end();
                if before.ends_with("typeof") {
                    start = abs + pattern.len();
                    continue;
                }

                let trimmed = line.trim_start();
                if trimmed.starts_with("//")
                    || trimmed.starts_with('*')
                    || trimmed.starts_with("/*")
                {
                    break;
                }

                violations.push(format!(
                    "Prefer `globalThis` over `{name}`. Replace `{name}.` with `globalThis.`."
                ));
                break;
            }
            start = abs + pattern.len();
        }
    }

    violations
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    // Projects declaring a browser/WebView-like runtime (VSCode extension,
    // Electron app, browser build target) may legitimately use `window` as
    // the real DOM Window object. Only fire on pure-Node projects.
    if project_allows_window(ctx.path) {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
        for msg in find_global_this_violations(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "prefer-global-this".into(),
                message: msg,
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::fs;
    use tempfile::TempDir;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    /// Build a temp project with an optional package.json body, then run
    /// the check on a source file placed inside `src/`. Returns the
    /// tempdir handle (so the caller keeps it alive) and the diagnostics.
    fn run_in_project(
        package_json: Option<&str>,
        source: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
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
        let d = run_ts("const url = window.location;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_self_in_worker() {
        let d = run_ts("self.fetch('/api');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn flags_global_process() {
        let d = run_ts("const env = global.process.env;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("globalThis"));
    }

    #[test]
    fn allows_global_this() {
        assert!(run_ts("const url = globalThis.location;").is_empty());
    }

    #[test]
    fn allows_window_specific_close() {
        assert!(run_ts("window.close();").is_empty());
    }

    #[test]
    fn allows_window_specific_inner_width() {
        assert!(run_ts("const w = window.innerWidth;").is_empty());
    }

    #[test]
    fn ignores_typeof_window() {
        assert!(run_ts("if (typeof window.x !== 'undefined') {}").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run_ts("// window.location is the URL").is_empty());
    }

    #[test]
    fn skips_when_engines_vscode_set() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "vscode": "^1.85.0" } }"#),
            "const x = window.foo;",
        );
        assert!(diags.is_empty(), "VSCode extension should not flag window.foo: {diags:?}");
    }

    #[test]
    fn skips_when_engines_electron_set() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "engines": { "electron": "^28.0.0" } }"#),
            "const x = window.foo;",
        );
        assert!(diags.is_empty(), "Electron app should not flag window.foo: {diags:?}");
    }

    #[test]
    fn skips_when_browserslist_present() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "browserslist": ["> 0.5%", "last 2 versions"] }"#),
            "const x = window.foo;",
        );
        assert!(diags.is_empty(), "Browser target should not flag window.foo: {diags:?}");
    }

    #[test]
    fn fires_on_pure_node_project() {
        let (_d, diags) = run_in_project(
            Some(r#"{ "dependencies": { "express": "^4.0.0" } }"#),
            "const x = window.foo;",
        );
        assert_eq!(diags.len(), 1, "Pure Node project should still flag window.foo: {diags:?}");
    }
}
