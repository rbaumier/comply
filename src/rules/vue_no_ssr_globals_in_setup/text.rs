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

/// Client/SSR-environment words that, leading a runtime boolean identifier
/// (case-insensitive), mark it as a guard (`clientOnly`, `browserOnly`, `ssrSafe`).
/// Such a variable is conventionally initialised from `typeof window !== "undefined"`,
/// so a branch it guards never runs server-side.
const RUNTIME_GUARD_WORDS: &[&str] = &["client", "browser", "ssr"];

/// Boolean/composable prefixes that, followed by a guard word, name a guard
/// (`isClient`, `isBrowser`, `isSSR`, `useBrowser`, `shouldRenderClient`).
const RUNTIME_GUARD_PREFIXES: &[&str] = &["is", "use", "can", "has", "should"];

/// True when `ident` reads as a runtime client/SSR guard variable by name: it leads
/// with a guard word (`clientOnly`), or with a boolean/composable prefix that is
/// followed somewhere by a guard word (`isClient`, `useBrowser`). Names that merely
/// *end* with a guard word (`apolloClient`, `httpClient`) are objects, not guards,
/// and are not matched.
fn is_runtime_guard_ident(ident: &str) -> bool {
    let lower = ident.to_ascii_lowercase();
    let leads_with_word = RUNTIME_GUARD_WORDS.iter().any(|w| lower.starts_with(w));
    let prefixed_guard = RUNTIME_GUARD_PREFIXES.iter().any(|p| {
        lower
            .strip_prefix(p)
            .is_some_and(|rest| RUNTIME_GUARD_WORDS.iter().any(|w| rest.contains(w)))
    });
    leads_with_word || prefixed_guard
}

/// True when the SSR global at byte offset `abs` is protected by a runtime
/// client/SSR guard earlier on the same line — a `isClient`/`isBrowser`-style
/// boolean used as a ternary condition or left-hand `&&` operand
/// (`isClient ? document… : null`, `isClient && window…`). The guarded branch is
/// only evaluated in the browser, so it never crashes during server render.
///
/// The guarding operator must directly follow the guard identifier (`isClient ?…`,
/// `isClient &&…`) and live in the same statement as the access — so an unrelated
/// mention, a `?`/`&&` from a separate `;`-terminated statement, or `?.`/`??` does
/// not suppress a real bare access.
fn runtime_guarded_before(line: &str, abs: usize) -> bool {
    // Only the access's own statement can guard it; a guard in an earlier
    // `;`-separated statement is unrelated.
    let stmt_start = line[..abs].rfind(';').map_or(0, |i| i + 1);
    let prefix = &line[stmt_start..abs];
    let bytes = prefix.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        // Skip past non-identifier-start bytes.
        if !(bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' || bytes[start] == b'$') {
            start += 1;
            continue;
        }
        let mut endp = start + 1;
        while endp < bytes.len()
            && (bytes[endp].is_ascii_alphanumeric() || bytes[endp] == b'_' || bytes[endp] == b'$')
        {
            endp += 1;
        }
        let ident = &prefix[start..endp];
        // A property access (`foo.isClient`) is not a standalone guard token.
        let preceded_by_dot = start > 0 && bytes[start - 1] == b'.';
        if is_runtime_guard_ident(ident) && !preceded_by_dot && guards_access(&prefix[endp..]) {
            return true;
        }
        start = endp;
    }
    false
}

/// True when `after` (the text immediately following the guard identifier) opens a
/// guarding operator: a logical `&&`, or a ternary `?` — excluding `?.` optional
/// chaining and `??` nullish coalescing, neither of which short-circuits the access.
fn guards_access(after: &str) -> bool {
    let rest = after.trim_start();
    if rest.starts_with("&&") {
        return true;
    }
    match rest.strip_prefix('?') {
        Some(tail) => !tail.starts_with('.') && !tail.starts_with('?'),
        None => false,
    }
}

/// True when the SSR global at byte offset `abs` is the operand of `typeof`
/// (`typeof window !== "undefined"`). `typeof` never throws on an undeclared
/// global, so this guard expression is itself SSR-safe.
fn is_typeof_operand(line: &str, abs: usize) -> bool {
    line[..abs]
        .trim_end()
        .strip_suffix("typeof")
        .is_some_and(|rest| rest.is_empty() || rest.ends_with(|c: char| !c.is_alphanumeric() && c != '_'))
}

/// VueUse composables that accept a `window`/`document` target as a call argument
/// and read it lazily inside an SSR-aware lifecycle hook. The composable internally
/// guards the access (`isClient`/`useSupported`) and no-ops during server render, so
/// passing a global directly is SSR-safe.
const SSR_AWARE_COMPOSABLES: &[&str] = &[
    "useEventListener",
    "useMutationObserver",
    "useResizeObserver",
    "useIntersectionObserver",
    "usePerformanceObserver",
    "useScroll",
];

/// True when the identifier matched at byte offset `abs` is the first argument to
/// a known SSR-aware composable call (`useEventListener(document, …)`). Such a
/// global is read lazily inside the composable's SSR-guarded hook, so it does not
/// crash during server render.
fn is_ssr_aware_composable_arg(line: &str, abs: usize) -> bool {
    let prefix = line[..abs].trim_end();
    let Some(prefix) = prefix.strip_suffix('(') else {
        return false;
    };
    let prefix = prefix.trim_end();
    SSR_AWARE_COMPOSABLES.iter().any(|name| match prefix.strip_suffix(name) {
        Some(rest) => rest.is_empty() || rest.ends_with(|c: char| !c.is_alphanumeric() && c != '_'),
        None => false,
    })
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
                    if !is_word
                        && !is_word_after
                        && !in_string[abs]
                        && !guarded_before(line, abs)
                        && !runtime_guarded_before(line, abs)
                        && !is_typeof_operand(line, abs)
                        && !is_ssr_aware_composable_arg(line, abs)
                    {
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
    fn allows_document_as_use_event_listener_arg() {
        // Issue #4735: element-plus notification.vue. `useEventListener` from
        // @vueuse/core is SSR-aware — it reads the `document` target lazily inside
        // an isClient-guarded hook, so passing it at the top level is safe.
        let sfc = "<script setup lang=\"ts\">\nuseEventListener(document, 'keydown', onKeydown)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_window_as_use_event_listener_arg() {
        let sfc = "<script setup>\nuseEventListener(window, 'resize', handler)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_bare_window_alongside_composable_arg() {
        // The SSR-aware composable arg is suppressed, but a genuine bare
        // `window.innerWidth` access on its own line is still flagged.
        let sfc = "<script setup>\nuseEventListener(window, 'resize', handler)\nconst w = window.innerWidth\n</script>";
        let diags = run(sfc);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`window`"));
    }

    #[test]
    fn flags_window_arg_to_unknown_composable() {
        // The allowlist is closed: a global passed to a composable not known to be
        // SSR-aware is still flagged, since its SSR behavior is unverified.
        let sfc = "<script setup>\nsomeOtherHook(window, 'resize', handler)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_isclient_ternary_guard() {
        // Issue #4769: ecomfe/vue-echarts demo/Demo.vue. `isClient` is a runtime
        // SSR guard (`typeof window !== "undefined"`); the `document` branch only
        // runs in the browser.
        let sfc = "<script setup lang=\"ts\">\nconst docRoot = isClient ? document.documentElement : null\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_isclient_logical_and_guard() {
        // Issue #4769: `isClient && window.location.hash` short-circuits, so
        // `window` is never evaluated during server render.
        let sfc = "<script setup lang=\"ts\">\nconst initialCodegenOpen = isClient && window.location.hash === \"#codegen\"\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_isbrowser_guard() {
        let sfc = "<script setup>\nconst t = isBrowser ? document.title : ''\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_typeof_window_check() {
        // `typeof window` never throws on an undeclared global, so the SSR guard
        // expression itself is safe.
        let sfc = "<script setup>\nconst isClient = typeof window !== \"undefined\"\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_unrelated_identifier_before_window() {
        // A non-guard identifier preceding the access (even with `&&`) must not
        // suppress a real bare `window` access.
        let sfc = "<script setup>\nconst w = ready && window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_isclient_without_guard_operator() {
        // `isClient` mentioned without a ternary/`&&` does not guard the access on
        // the same line.
        let sfc = "<script setup>\nconst x = isClient; const w = window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_nullish_coalescing_is_not_a_guard() {
        // `??` is nullish coalescing, not a ternary; it does not short-circuit the
        // `window` access, so a guard-named left operand must not suppress it.
        let sfc = "<script setup>\nconst origin = browserUrl ?? window.location.origin\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_optional_chaining_is_not_a_guard() {
        // `clientRef?.x` is optional chaining, not a ternary; the `?` directly after
        // the guard-named identifier must not be read as a guard.
        let sfc = "<script setup>\nconst w = clientRef?.x; const z = window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_object_named_client_is_not_a_guard() {
        // An identifier that merely ends with `Client` (`apolloClient`) is an
        // object, not an SSR guard, so it must not suppress a real access.
        let sfc = "<script setup>\nconst el = apolloClient ? document.body : null\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_guard_operator_from_separate_statement() {
        // A ternary in an earlier `;`-separated statement does not guard a later
        // bare `window` access.
        let sfc = "<script setup>\nconst a = isClient ? 1 : 0; const w = window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
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
