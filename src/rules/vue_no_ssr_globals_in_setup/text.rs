//! vue-no-ssr-globals-in-setup AST backend.
//!
//! Applies only to projects that do Vue server-side rendering (Nuxt, or a Vue
//! SSR renderer dependency). A pure client-side SPA never renders on the server,
//! so top-level access to these globals is safe there and is not flagged.

use crate::diagnostic::{Diagnostic, Severity};

const SSR_GLOBALS: &[&str] = &[
    "window",
    "document",
    "localStorage",
    "sessionStorage",
    "navigator",
];

/// Vite/Nuxt compile-time SSR guards. When one of these appears before an SSR
/// global on the same line, the bundler statically replaces it (`false` during
/// SSR), so the guarded branch is never evaluated server-side and is SSR-safe.
const SSR_GUARDS: &[&str] = &[
    "import.meta.client",
    "import.meta.server",
    "process.client",
    "process.server",
];

/// True when a static SSR guard token appears on `line` before byte offset `abs`.
fn guarded_before(line: &str, abs: usize) -> bool {
    SSR_GUARDS
        .iter()
        .filter_map(|guard| line.find(guard))
        .any(|guard_at| guard_at < abs)
}

/// JS binding keywords that introduce a local variable.
const DECL_KEYWORDS: &[&str] = &["const", "let", "var"];

/// True when the identifier matched at byte offset `abs` is the name being bound
/// by a local declaration (`const window = useWindow()`, `let document = …`),
/// including a single destructured name (`const { window } = …`,
/// `const [document] = …`). Such a name shadows the browser global, so subsequent
/// references on later lines are the local binding, not the SSR-unsafe global.
fn is_local_declaration(line: &str, abs: usize) -> bool {
    let prefix = line[..abs].trim_end();
    // Step past an opening destructuring delimiter, if any.
    let prefix = prefix
        .strip_suffix('{')
        .or_else(|| prefix.strip_suffix('['))
        .map(str::trim_end)
        .unwrap_or(prefix);
    DECL_KEYWORDS.iter().any(|kw| match prefix.strip_suffix(kw) {
        Some(rest) => rest.is_empty() || rest.ends_with(|c: char| !c.is_alphanumeric() && c != '_'),
        None => false,
    })
}

/// Per-byte mask of `line` where `true` marks a byte inside a string literal
/// (`'…'`, `"…"`, or `` `…` ``), respecting backslash escapes. Quote delimiters
/// themselves are marked too, so a global word matched anywhere in the span is
/// treated as string content (prose), not a code-context global access.
fn string_literal_mask(line: &str) -> Vec<bool> {
    let bytes = line.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) => {
                mask[i] = true;
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == q {
                    quote = None;
                }
            }
            None => {
                if b == b'\'' || b == b'"' || b == b'`' {
                    quote = Some(b);
                    mask[i] = true;
                }
            }
        }
    }
    mask
}

fn script_setup_range(source: &str) -> Option<(usize, usize)> {
    for (i, _) in source.match_indices("<script") {
        let close = source[i..].find('>')?;
        let tag = &source[i..i + close];
        if tag.contains("setup") {
            let body_start = i + close + 1;
            let end_rel = source[body_start..].find("</script>")?;
            return Some((body_start, body_start + end_rel));
        }
    }
    None
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    // SSR-only concern: top-level `window`/`document` crashes during server
    // render. A pure client-side SPA (no Nuxt, no Vue SSR renderer) never
    // renders on the server, so the access is safe there — only flag
    // SSR-capable projects.
    if !ctx.project.uses_vue_ssr() {
        return;
    }
    let Some((start, end)) = script_setup_range(ctx.source) else {
        return;
    };
    let body = &ctx.source[start..end];
    let base_line = ctx.source[..start].matches('\n').count();
    let mut depth = 0i32;
    // Globals re-bound to a local variable earlier in the block (e.g.
    // `const window = useWindow()`). Subsequent uses reference the local, not the
    // SSR-unsafe global, so they must not be flagged.
    let mut shadowed: Vec<&str> = Vec::new();
    for (idx, line) in body.lines().enumerate() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("//") {
            continue;
        }
        if depth == 0 {
            let in_string = string_literal_mask(line);
            for g in SSR_GLOBALS {
                if shadowed.contains(g) {
                    continue;
                }
                let mut pos = 0;
                while let Some(p) = line[pos..].find(g) {
                    let abs = pos + p;
                    let before = if abs == 0 { ' ' } else { line.as_bytes()[abs - 1] as char };
                    let after = line.as_bytes().get(abs + g.len()).map(|b| *b as char).unwrap_or(' ');
                    let is_word = before.is_alphanumeric() || before == '_' || before == '.';
                    let is_word_after = after.is_alphanumeric() || after == '_';
                    if !is_word && !is_word_after && !in_string[abs] && is_local_declaration(line, abs) {
                        shadowed.push(g);
                        break;
                    }
                    if !is_word && !is_word_after && !in_string[abs] && !guarded_before(line, abs) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: base_line + idx + 1,
                            column: abs + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{g}` at the top of `<script setup>` crashes during SSR. Wrap in `onMounted(() => {{ ... }})`."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        break;
                    }
                    pos = abs + g.len();
                }
            }
        }
        for b in line.bytes() {
            match b {
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth -= 1,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::{AstCheck, CheckCtx};

    /// Run the rule against a `.vue` file inside a tempdir whose `package.json`
    /// is `pkg_json`, with a real `ProjectCtx` so the SSR gate (`uses_vue_ssr`)
    /// can read framework detection and declared dependencies from it.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join("t.vue");
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::Vue,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(
            &CheckCtx::for_test_with_project(&canon, source, &project),
            &tree,
        )
    }

    /// `package.json` of a Vue SSR project: declares the SSR renderer, so
    /// `uses_vue_ssr()` is true and top-level global access is flagged. The
    /// positive cases run under this manifest.
    const SSR_PKG: &str = r#"{ "dependencies": { "vue": "^3.4.0", "@vue/server-renderer": "^3.4.0" } }"#;

    /// `package.json` of a pure client-side Vue SPA (koel shape): `vue` plus the
    /// Vite plugin, no SSR renderer and no Nuxt. `uses_vue_ssr()` is false here,
    /// so top-level global access is safe and must not be flagged.
    const SPA_PKG: &str = r#"{ "dependencies": { "vue": "^3.4.0" }, "devDependencies": { "@vitejs/plugin-vue": "^5.0.0" } }"#;

    /// Positive cases run under a Vue SSR project, where top-level `window` /
    /// `document` access really crashes during server render.
    fn run(source: &str) -> Vec<Diagnostic> {
        run_with_pkg(SSR_PKG, source)
    }

    #[test]
    fn flags_window_top_level() {
        let sfc = "<script setup>\nconst w = window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_document_top_level() {
        let sfc = "<script setup>\nconst t = document.title\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_window_in_onmounted() {
        let sfc = "<script setup>\nonMounted(() => {\n  const w = window.innerWidth\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_plain_script() {
        let sfc = "<script>\nconst w = window.innerWidth\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_import_meta_client_ternary() {
        // Issue #3308: import.meta.client is a compile-time SSR guard; the
        // truthy branch is never evaluated server-side.
        let sfc = "<script setup>\nconst appendToBody = import.meta.client ? () => document.body : undefined\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_import_meta_server_ternary() {
        // window access sits in the false (CSR) branch; the line-level guard
        // still recognizes it as SSR-safe.
        let sfc = "<script setup>\nconst x = import.meta.server ? undefined : window.innerWidth\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_process_client_ternary() {
        let sfc = "<script setup>\nconst w = process.client ? window.scrollY : 0\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_unguarded_window() {
        let sfc = "<script setup>\nconst w = window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_window_when_guard_comes_after() {
        // The global access precedes the guard token on the line, so it is not
        // protected by it.
        let sfc = "<script setup>\nconst w = window.innerWidth // import.meta.client\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn ignores_global_word_in_single_quoted_string() {
        // Issue #3888: prose inside a string literal is not a global access.
        let sfc = "<script setup>\nconst s = 'click outside of the document'\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_global_word_in_double_quoted_string() {
        let sfc = "<script setup>\nconst t = \"window resize handler\"\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_global_word_in_backtick_string() {
        let sfc = "<script setup>\nconst u = `navigator info`\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_only_real_access_when_line_mixes_string_and_code() {
        // The string mention of `document` is prose; only the real `window`
        // access (in code context) is flagged, at its column.
        let sfc = "<script setup>\nconst x = 'document'; const w = window.innerWidth\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`window`"));
        // `window` starts at byte offset 32 on the line (column 33, 1-based).
        assert_eq!(diags[0].column, 33);
    }

    #[test]
    fn skips_pure_spa_project() {
        // Issue #4499: phanan/koel is a Laravel + Vue SPA (vue + @vitejs/plugin-vue,
        // no Nuxt, no SSR renderer). Top-level `window` access is safe in a
        // client-only build, so the rule must not fire there.
        let sfc = "<script lang=\"ts\" setup>\nconst demoAccount = window.KOEL.demo_account || {}\n</script>";
        assert!(run_with_pkg(SPA_PKG, sfc).is_empty());
    }

    #[test]
    fn allows_window_shadowed_by_local_composable() {
        // Issue #4674: epicmaxco/vuestic-ui VaSlider.vue. `window` is re-bound to
        // the SSR-safe `useWindow()` composable; later `useEvent(..., window)`
        // calls reference the local, not the browser global.
        let sfc = "<script setup lang=\"ts\">\nconst window = useWindow()\nuseEvent(['mousemove', 'touchmove'], moving, window)\nuseEvent(['mouseup', 'mouseleave'], moveEnd, window)\nuseEvent('keydown', moveWithKeys, window)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_document_shadowed_by_local_declaration() {
        let sfc = "<script setup>\nconst document = useDocument()\nconst el = document.body\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_window_still_when_not_shadowed() {
        // A later shadow of `document` must not suppress an earlier raw `window`.
        let sfc = "<script setup>\nconst w = window.innerWidth\nconst document = useDocument()\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`window`"));
    }

    #[test]
    fn flags_use_before_shadow_declaration() {
        // A bare global access on a line before its local declaration is still the
        // browser global at that point.
        let sfc = "<script setup>\nconst w = window.innerWidth\nconst window = useWindow()\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn shadow_detection_only_at_top_level() {
        // A `const window` inside a nested block is not a top-level binding, so it
        // must not suppress a sibling top-level `window` access. Top-level scanning
        // only happens at brace-depth 0, so the nested declaration is ignored and
        // the real top-level global is still flagged.
        let sfc = "<script setup>\nfunction f() {\n  const window = useWindow()\n}\nconst w = window.innerWidth\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`window`"));
    }

    #[test]
    fn flags_in_nuxt_project() {
        // Nuxt does SSR, so `uses_vue_ssr()` is true via framework detection and
        // top-level `window` access is still flagged.
        let pkg = r#"{ "dependencies": { "nuxt": "^3.11.0" } }"#;
        let sfc = "<script setup>\nconst w = window.innerWidth\n</script>";
        assert_eq!(run_with_pkg(pkg, sfc).len(), 1);
    }
}
