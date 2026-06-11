// Infrastructure landing ahead of consumers: chantier #1 ships the
// ProjectCtx/FileCtx scaffolding, chantiers #2+ migrate rules onto it.
#![allow(dead_code)]

//! Per-file context built once per file in `dispatch_backends`.
//!
//! Operator consequence: rules that want "is this a server component?", "am
//! I in a test file?", or "does this file have a `use client` directive?"
//! read `ctx.file.*` instead of re-scanning the source or re-parsing the
//! path on every rule.
//!
//! How:
//! - `scan_directives` walks the first bytes of the source skipping
//!   whitespace and comments, accepting up to two top-level string
//!   expression statements as RSC directives (`"use client"` / `"use server"`).
//! - `scan_path` is pure path manipulation — no IO.
//! - `classify_rsc` combines directives + framework + path segments.

use std::path::Path;
use std::sync::OnceLock;

use crate::files::Language;
use crate::project::{Framework, ProjectCtx};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileDirectives {
    pub use_client: bool,
    pub use_server: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RscContext {
    ServerComponent,
    ClientComponent,
    ServerFunction,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathSegments {
    pub in_app_router: bool,
    pub in_pages_router: bool,
    pub in_test_dir: bool,
    pub in_node_modules: bool,
    pub in_storybook: bool,
    pub is_vendored: bool,
    /// `examples/`, `benches/`, or `fixtures/` anywhere in the path —
    /// directories where api/rust/security rules are intentionally relaxed.
    pub is_relaxed_dir: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FileCtx {
    pub language: Option<Language>,
    pub directives: FileDirectives,
    pub rsc_context: RscContext,
    pub path_segments: PathSegments,
    pub is_generated: bool,
    pub is_minified: bool,
}

impl FileCtx {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn build(path: &Path, source: &str, language: Language, project: &ProjectCtx) -> Self {
        let directives = scan_directives(source);
        let path_segments = scan_path(path);
        let rsc_context = classify_rsc(project.framework, directives, &path_segments);
        let is_generated = scan_generated(source);
        let is_minified = scan_minified(path, source);
        FileCtx {
            language: Some(language),
            directives,
            rsc_context,
            path_segments,
            is_generated,
            is_minified,
        }
    }
}

/// Scan the start of the source for one or two top-level string expression
/// statements. `"use strict"`, `"use client"`, `"use server"` are all valid
/// JS directives in the TC39 sense.
fn scan_directives(source: &str) -> FileDirectives {
    let mut out = FileDirectives::default();
    let bytes = source.as_bytes();
    let mut cursor = skip_ws_comments(bytes, 0);
    for _ in 0..2 {
        if cursor >= bytes.len() {
            break;
        }
        let Some((value, end)) = read_string_stmt(bytes, cursor) else {
            break;
        };
        match value {
            "use client" => out.use_client = true,
            "use server" => out.use_server = true,
            _ => {}
        }
        cursor = skip_ws_comments(bytes, end);
    }
    out
}

/// Read a `"…"` or `'…'` string literal followed by `;` or a newline.
fn read_string_stmt(bytes: &[u8], start: usize) -> Option<(&str, usize)> {
    let quote = *bytes.get(start)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        if bytes[cursor] == quote {
            let value = std::str::from_utf8(&bytes[start + 1..cursor]).ok()?;
            let mut tail = cursor + 1;
            while tail < bytes.len() && (bytes[tail] == b' ' || bytes[tail] == b'\t') {
                tail += 1;
            }
            if tail >= bytes.len() {
                return Some((value, tail));
            }
            if bytes[tail] == b';' || bytes[tail] == b'\n' || bytes[tail] == b'\r' {
                return Some((value, tail + 1));
            }
            return None;
        }
        cursor += 1;
    }
    None
}

fn skip_ws_comments(bytes: &[u8], mut cursor: usize) -> usize {
    loop {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor + 1 < bytes.len() && bytes[cursor] == b'/' && bytes[cursor + 1] == b'/' {
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            continue;
        }
        if cursor + 1 < bytes.len() && bytes[cursor] == b'/' && bytes[cursor + 1] == b'*' {
            cursor += 2;
            while cursor + 1 < bytes.len() && !(bytes[cursor] == b'*' && bytes[cursor + 1] == b'/')
            {
                cursor += 1;
            }
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        return cursor;
    }
}

fn scan_generated(source: &str) -> bool {
    let end = source.floor_char_boundary(2048);
    let head = &source[..end];
    for line in head.lines().take(30) {
        let trimmed = line.trim();
        if !(trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with('#')
            || trimmed.starts_with("<!--"))
        {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("@generated")
            || lower.contains("auto-generated")
            || lower.contains("do not edit")
            || lower.contains("do not make direct changes")
            || lower.contains("this file is automatically generated")
        {
            return true;
        }
    }
    false
}

fn scan_minified(path: &Path, source: &str) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !matches!(ext, "js" | "css" | "mjs" | "cjs") {
        return false;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.contains(".min.") {
        return true;
    }
    // Heuristic: minified content is either a handful of lines, or a normal
    // line count with one machine-generated line that dwarfs the rest. Bundles
    // often keep a few short header lines above a single multi-KB payload line.
    if source.len() > 4096 {
        let line_count = source.bytes().filter(|&b| b == b'\n').count() + 1;
        if line_count <= 3 || source.split('\n').any(|line| line.len() > 4096) {
            return true;
        }
    }
    false
}

/// Vendored directory names — matched as exact path segments (between `/`
/// delimiters) so that e.g. `vendor-service/` is NOT considered vendored.
const VENDORED_SEGMENTS: &[&str] = &[
    "vendor",
    "vendors",
    "vendored",
    "external",
    "third-party",
    "third_party",
];

fn has_vendored_segment(normalized: &str) -> bool {
    normalized.split('/').any(|seg| VENDORED_SEGMENTS.contains(&seg))
}

fn scan_path(path: &Path) -> PathSegments {
    let lower = path.to_string_lossy().replace('\\', "/");
    PathSegments {
        in_app_router: lower.contains("/app/") || lower.starts_with("app/"),
        in_pages_router: lower.contains("/pages/") || lower.starts_with("pages/"),
        in_test_dir: lower.contains("/tests/")
            || lower.contains("/test/")
            || lower.contains("/tests-")
            || lower.contains("-tests/")
            || lower.contains("/test-helpers/")
            || lower.contains("/test-helper/")
            || lower.starts_with("tests/")
            || lower.starts_with("test/")
            || lower.starts_with("tests-")
            || lower.starts_with("test-helpers/")
            || lower.starts_with("test-helper/")
            || lower.contains("/test-d/")
            || lower.starts_with("test-d/")
            || lower.contains("/dtslint/")
            || lower.starts_with("dtslint/")
            || lower.contains("/__tests__/")
            || lower.starts_with("__tests__/")
            || lower.contains("/fixtures/")
            || lower.contains("/__mocks__/")
            || lower.contains(".test.")
            || lower.contains(".spec.")
            || lower.ends_with("/test.ts")
            || lower.ends_with("/test.tsx")
            || lower.ends_with("/test.js")
            || lower.ends_with("/test.jsx")
            || lower == "test.ts"
            || lower == "test.tsx"
            || lower == "test.js"
            || lower == "test.jsx",
        in_node_modules: lower.contains("/node_modules/"),
        in_storybook: lower.contains(".stories."),
        is_vendored: has_vendored_segment(&lower),
        is_relaxed_dir: lower.starts_with("examples/")
            || lower.starts_with("benches/")
            || lower.starts_with("fixtures/")
            || lower.contains("/examples/")
            || lower.contains("/benches/")
            || lower.contains("/fixtures/"),
    }
}

fn classify_rsc(
    framework: Framework,
    directives: FileDirectives,
    segments: &PathSegments,
) -> RscContext {
    if directives.use_server {
        return RscContext::ServerFunction;
    }
    if directives.use_client {
        return RscContext::ClientComponent;
    }
    if framework == Framework::NextJs && segments.in_app_router {
        return RscContext::ServerComponent;
    }
    RscContext::Unknown
}

#[cfg(test)]
pub(crate) fn default_static_file_ctx() -> &'static FileCtx {
    static DEFAULT: OnceLock<FileCtx> = OnceLock::new();
    DEFAULT.get_or_init(FileCtx::empty)
}

// Keep the `OnceLock` import live outside of cfg(test) without warnings;
// a future non-test consumer of the static default will drop this.
#[cfg(not(test))]
#[allow(dead_code)]
fn _keep_once_lock_in_scope() -> OnceLock<()> {
    OnceLock::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn scans_use_client_directive() {
        let directives = scan_directives("\"use client\";\nexport function X() {}");
        assert!(directives.use_client);
        assert!(!directives.use_server);
    }

    #[test]
    fn scans_use_server_with_single_quotes() {
        let directives = scan_directives("'use server';\nfoo();");
        assert!(directives.use_server);
    }

    #[test]
    fn skips_leading_block_comment() {
        let directives = scan_directives("/* license */\n\"use client\";\n");
        assert!(directives.use_client);
    }

    #[test]
    fn skips_leading_line_comment() {
        let directives = scan_directives("// hi\n\"use server\"\n");
        assert!(directives.use_server);
    }

    #[test]
    fn accepts_two_directives_in_sequence() {
        let directives = scan_directives("\"use strict\";\n\"use client\";\n");
        assert!(directives.use_client);
    }

    #[test]
    fn no_directive_when_absent() {
        let directives = scan_directives("export function X() {}");
        assert!(!directives.use_client);
        assert!(!directives.use_server);
    }

    #[test]
    fn does_not_match_string_inside_expression() {
        let directives = scan_directives("const x = \"use client\";");
        assert!(!directives.use_client);
    }

    #[test]
    fn minified_long_single_line_among_normal_lines() {
        // A few short lines above one multi-KB payload line — the shape of a
        // bundle that keeps header lines above the minified body.
        let long = "a".repeat(5000);
        let src = format!("// header\nconst x = 1;\nconst data = \"{long}\";\n");
        assert!(scan_minified(&PathBuf::from("dist/assets/index-abc123.js"), &src));
    }

    #[test]
    fn not_minified_normal_multiline_js() {
        let src = "const a = 1;\nconst b = 2;\nfunction f() {\n  return a + b;\n}\n".repeat(200);
        assert!(!scan_minified(&PathBuf::from("src/app.js"), &src));
    }

    #[test]
    fn long_line_in_ts_is_not_minified() {
        // The minified heuristic only applies to js/css bundle extensions.
        let long = "a".repeat(5000);
        let src = format!("const data = \"{long}\";\n");
        assert!(!scan_minified(&PathBuf::from("src/data.ts"), &src));
    }

    #[test]
    fn path_app_router() {
        let seg = scan_path(&PathBuf::from("/proj/app/page.tsx"));
        assert!(seg.in_app_router);
        assert!(!seg.in_pages_router);
    }

    #[test]
    fn path_test_markers() {
        assert!(scan_path(&PathBuf::from("src/foo.test.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/foo.spec.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("__tests__/foo.ts")).in_test_dir);
        // Test-helper infrastructure directories (issue #481).
        assert!(scan_path(&PathBuf::from("src/api/test-helpers/als-proxy.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/test-helper/db.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("test-helpers/setup.ts")).in_test_dir);
        // tsd type-testing convention (issue #793).
        assert!(scan_path(&PathBuf::from("test-d/schema.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/test-d/types.ts")).in_test_dir);
        // dtslint type-testing convention (issue #1006).
        assert!(scan_path(&PathBuf::from("dtslint/Array.ts")).in_test_dir);
        assert!(
            scan_path(&PathBuf::from(
                "very-well-written-projects/typescript/fp-ts/dtslint/Array.ts"
            ))
            .in_test_dir
        );
    }

    #[test]
    fn hyphenated_plural_tests_dirs_are_test_dirs() {
        // `*-tests/` segments such as integration-tests/ or type-tests/ (issue #979).
        assert!(
            scan_path(&PathBuf::from("integration-tests/type-tests/join-nodenext/mysql.ts"))
                .in_test_dir
        );
        // Singular `*-test/` stays a feature dir (e.g. A/B testing), not a test dir.
        assert!(!scan_path(&PathBuf::from("src/ab-test/widget.ts")).in_test_dir);
    }

    #[test]
    fn vendored_exact_segments() {
        assert!(scan_path(&PathBuf::from("lib/vendor/foo.js")).is_vendored);
        assert!(scan_path(&PathBuf::from("lib/vendors/foo.js")).is_vendored);
        assert!(scan_path(&PathBuf::from("lib/vendored/foo.js")).is_vendored);
        assert!(scan_path(&PathBuf::from("server/core/static/external/base64.js")).is_vendored);
        assert!(scan_path(&PathBuf::from("lib/third-party/confetti.js")).is_vendored);
        assert!(scan_path(&PathBuf::from("lib/third_party/confetti.js")).is_vendored);
    }

    #[test]
    fn vendored_at_path_root() {
        assert!(scan_path(&PathBuf::from("vendor/foo.js")).is_vendored);
        assert!(scan_path(&PathBuf::from("external/bar.css")).is_vendored);
    }

    #[test]
    fn vendored_does_not_match_substrings() {
        assert!(!scan_path(&PathBuf::from("vendor-service/api.ts")).is_vendored);
        assert!(!scan_path(&PathBuf::from("src/my-vendor-lib/foo.ts")).is_vendored);
        assert!(!scan_path(&PathBuf::from("src/externalize/foo.ts")).is_vendored);
    }

    #[test]
    fn normal_files_not_vendored() {
        assert!(!scan_path(&PathBuf::from("src/app.ts")).is_vendored);
        assert!(!scan_path(&PathBuf::from("lib/utils.js")).is_vendored);
    }

    #[test]
    fn rsc_server_component_in_app_under_next() {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        let ctx = FileCtx::build(
            Path::new("/proj/app/page.tsx"),
            "export default function Page() {}",
            Language::Tsx,
            &project,
        );
        assert_eq!(ctx.rsc_context, RscContext::ServerComponent);
    }

    #[test]
    fn rsc_client_component_via_directive() {
        let project = ProjectCtx::empty();
        let ctx = FileCtx::build(
            Path::new("/proj/src/page.tsx"),
            "\"use client\";\nexport default function X() {}",
            Language::Tsx,
            &project,
        );
        assert_eq!(ctx.rsc_context, RscContext::ClientComponent);
    }
}
