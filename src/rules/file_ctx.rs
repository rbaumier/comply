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
    /// An `internal/` directory directly under a test root (`test/internal/`,
    /// `tests/internal/`, `__tests__/internal/`, `_tests_/internal/`). By the
    /// Azure-SDK convention these tests deliberately exercise — and mock —
    /// internal implementation details, unlike `test/public/` tests that
    /// verify the public API contract.
    pub in_test_internal_dir: bool,
    pub in_node_modules: bool,
    pub in_storybook: bool,
    pub is_vendored: bool,
    /// `examples/`, `example/`, `demo/`, `demos/`, `benches/`, `fixtures/`,
    /// `samples/`, or `docs/` anywhere in the path — directories where
    /// api/rust/security rules are intentionally relaxed and where intentional
    /// duplication (multi-bundler/standalone demos) is documentation, not a
    /// smell.
    pub is_relaxed_dir: bool,
    /// An auxiliary, non-shipped directory segment (scripts/bin/config/
    /// migrations/samples/examples/templates/scaffold/boilerplate). The broad
    /// set consumed by `no-extraneous-import`; see
    /// [`crate::rules::path_utils::is_aux_dir_path`].
    pub in_aux_dir: bool,
    /// A cargo-fuzz `fuzz_targets/` directory segment — where `panic!` is the
    /// deliberate crash-signaling mechanism.
    pub in_fuzz_targets: bool,
    /// A benchmark file: a `benches/` directory segment (Rust's convention) or a
    /// `_bench`/`-bench`/`.bench.` file-stem marker. Benchmark setup/teardown
    /// deliberately uses fast, side-stepping operations (e.g. `TRUNCATE` to reset
    /// tables between iterations) that production rules must not flag.
    pub in_benchmark_dir: bool,
    /// A test-runner setup/teardown hook file resolved by path, not by name:
    /// Vitest `globalSetup`/`setup` files and Playwright
    /// `globalSetup`/`globalTeardown` files. The framework imports them by the
    /// configured file path and invokes their default export, so the default
    /// export is anonymous by convention (the file name carries the intent).
    pub is_framework_hook_file: bool,
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

    /// True when this file is benchmark code (a `benches/` directory or a
    /// `_bench`/`-bench`/`.bench.` file marker). Opt-in: consulted only by rules
    /// that target production application code and must not fire on benchmark
    /// setup/teardown.
    pub fn in_benchmark_dir(&self) -> bool {
        self.path_segments.in_benchmark_dir
    }

    pub fn build(path: &Path, source: &str, language: Language, project: &ProjectCtx) -> Self {
        let directives = scan_directives(source);
        let path_segments = scan_path(path);
        let rsc_context = classify_rsc(project.framework, directives, &path_segments);
        let is_generated =
            is_generated_content(source) || is_generated_filename(path) || is_in_generated_dir(path);
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

/// True when the head of `source` carries a blanket codegen marker: a bare
/// whole-file `eslint-disable` (no rule list), a `@generated` / `do not edit`
/// / `automatically generated` / `code generated` banner, or an AutoRest-style
/// `contains only generated` self-declaration. The shared content
/// predicate behind both the engine gate (via [`FileCtx::is_generated`]) and
/// `clone_detection`, so a content-marked file is exempt from every rule even
/// when its name and directory carry no codegen signal.
pub(crate) fn is_generated_content(source: &str) -> bool {
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
            || lower.contains("automatically generated")
            || lower.contains("code generated")
            // AutoRest / TypeSpec emit a file-level self-declaration banner
            // (e.g. "This file contains only generated model types and their
            // (de)serializers.") with no `@generated` / `do not edit` marker.
            || lower.contains("contains only generated")
        {
            return true;
        }
        // A bare whole-file ESLint disable (no rule list) is a codegen header
        // convention (e.g. TanStack Router); a targeted `eslint-disable <rule>`
        // is hand-written and must not match.
        let inner = trimmed
            .trim_start_matches("/*")
            .trim_start_matches("//")
            .trim_end_matches("*/")
            .trim();
        if inner == "eslint-disable" {
            return true;
        }
        // The Protobuf compiler emits a targeted `eslint-disable` header that
        // includes `no-prototype-builtins` (its output uses `.hasOwnProperty()`
        // extensively). That rule list is a codegen signature, not a
        // hand-written suppression.
        if let Some(rules) = inner.strip_prefix("eslint-disable ")
            && rules.split(',').any(|r| r.trim() == "no-prototype-builtins")
        {
            return true;
        }
    }
    false
}

/// Filename suffixes used by codegen tools for generated source files
/// (e.g. TanStack Router's `routeTree.gen.ts`). Matched against the
/// lowercased file name, so `mygen.ts` does not match `.gen.ts`.
const GENERATED_FILENAME_SUFFIXES: &[&str] = &[
    ".gen.ts",
    ".gen.tsx",
    ".gen.js",
    ".gen.jsx",
    ".gen.mts",
    ".gen.cts",
    ".generated.ts",
    ".generated.tsx",
    ".generated.js",
    ".generated.jsx",
];

fn is_generated_filename(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    GENERATED_FILENAME_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix))
}

/// A `generated/` directory anywhere in the path — matched as an exact path
/// segment (between `/` delimiters) so that e.g. `generated-utils/` is NOT
/// treated as a codegen output directory. Files here are emitted by a generator
/// and cannot be hand-edited (e.g. Protobuf output under `src/generated/`).
fn is_in_generated_dir(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized.split('/').any(|seg| seg == "generated")
}

/// True when `path` is recognized as generated from its path alone — either a
/// codegen filename suffix (e.g. `routeTree.gen.ts`) or a `generated/` directory
/// segment. The content-based `@generated`-marker scan ([`is_generated_content`]) is
/// intentionally excluded: this predicate is for callers that only have a path,
/// not the file source.
pub(crate) fn is_generated_path(path: &Path) -> bool {
    is_generated_filename(path) || is_in_generated_dir(path)
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

/// Storybook directory names — matched as exact path segments (between `/`
/// delimiters) so that e.g. `mystories.ts` or `storybook-static/` is NOT
/// considered a story-content directory. Catches helper/story files that live
/// inside a `stories/` or `storybook/` directory without a `.stories.` name.
const STORYBOOK_SEGMENTS: &[&str] = &["stories", "storybook"];

fn has_storybook_segment(normalized: &str) -> bool {
    normalized.split('/').any(|seg| STORYBOOK_SEGMENTS.contains(&seg))
}

/// Relaxed directory names — matched as exact path segments (between `/`
/// delimiters) so that e.g. `documentation/` or `example-app/` is NOT relaxed.
/// These hold demonstration scripts, fixtures, benchmarks, and docs where
/// stricter conventions are intentionally relaxed and intentional duplication
/// is documentation, not a smell.
const RELAXED_SEGMENTS: &[&str] = &[
    "examples",
    "example",
    "demo",
    "demos",
    "benches",
    "fixtures",
    "__fixtures__",
    "samples",
    "docs",
];

fn has_relaxed_segment(normalized: &str) -> bool {
    normalized.split('/').any(|seg| RELAXED_SEGMENTS.contains(&seg))
}

/// Conventional file stems for test-runner setup/teardown hooks that the
/// framework resolves by path and invokes via the file's default export
/// (Vitest `globalSetup`/`setup`, Playwright `globalSetup`/`globalTeardown`).
/// Matched against the lowercased file stem so a longer identifier such as
/// `setupRouter.ts` does not match. The framework names the entry point by
/// file path, so the default export is anonymous by convention.
const FRAMEWORK_HOOK_STEMS: &[&str] = &[
    "setup",
    "global-setup",
    "globalsetup",
    "teardown",
    "global-teardown",
    "globalteardown",
];

/// True when `path` is a test-runner setup/teardown hook file by name (see
/// [`FRAMEWORK_HOOK_STEMS`]). The stem must equal a hook name exactly, so
/// `setupRouter.ts` (a regular module) is not matched.
fn is_framework_hook_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    FRAMEWORK_HOOK_STEMS.contains(&stem.as_str())
}

/// True for a file in an integration-test fixture application: an `integration/`
/// directory segment followed (at any later depth) by a `src/` segment, i.e. the
/// `integration/<app>/src/...` shape. Monorepos (NestJS, Angular, TypeORM) keep
/// full mini-applications there that exist solely to be spun up by sibling
/// `e2e/*.spec.ts` integration-test runners; they are test scaffolding, never
/// published, so devDependency imports from them are correct. The trailing `src/`
/// requirement keeps a production `src/integration/` module (an "integration with
/// service X", where `src` precedes `integration` and no nested `src` follows)
/// from matching.
fn has_integration_fixture_app_shape(normalized: &str) -> bool {
    let segments: Vec<&str> = normalized.split('/').collect();
    let Some(integration_idx) = segments.iter().position(|seg| *seg == "integration") else {
        return false;
    };
    segments[integration_idx + 1..].iter().any(|seg| *seg == "src")
}

/// Test-root directory names that pair with an `internal/` child to form the
/// `test/internal/` convention — matched as exact path segments.
const TEST_ROOT_SEGMENTS: &[&str] = &["test", "tests", "__tests__", "_tests_"];

/// An `internal/` directory directly under a test root (e.g. `test/internal/`).
fn has_test_internal_dir(normalized: &str) -> bool {
    let segments: Vec<&str> = normalized.split('/').collect();
    segments
        .windows(2)
        .any(|pair| TEST_ROOT_SEGMENTS.contains(&pair[0]) && pair[1] == "internal")
}

/// True when `path` is a benchmark source file: a `benches/` directory segment
/// (Rust's standard benchmark directory, matched between `/` delimiters so
/// `benches-old/` is not matched) or a file stem carrying a `_bench`/`-bench`
/// marker (e.g. `parse_bench.rs`), or a `.bench.` filename infix
/// (e.g. `parse.bench.ts`).
fn is_benchmark_path(normalized: &str) -> bool {
    if normalized.split('/').any(|seg| seg == "benches") {
        return true;
    }
    let Some(name) = normalized.rsplit('/').next() else {
        return false;
    };
    if name.contains(".bench.") {
        return true;
    }
    let stem = name.split('.').next().unwrap_or("");
    stem.ends_with("_bench") || stem.ends_with("-bench")
}

pub(crate) fn scan_path(path: &Path) -> PathSegments {
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
            || lower.contains("/test-vr/")
            || lower.starts_with("test-vr/")
            || lower.starts_with("tests/")
            || lower.starts_with("test/")
            || lower.starts_with("tests-")
            || lower.starts_with("test-helpers/")
            || lower.starts_with("test-helper/")
            || lower.contains("/test-d/")
            || lower.starts_with("test-d/")
            || lower.contains("/test-tsd/")
            || lower.starts_with("test-tsd/")
            || lower.contains("/spec/")
            || lower.starts_with("spec/")
            || crate::rules::path_utils::has_test_d_infix(path)
            || crate::rules::path_utils::has_type_probe_infix(path)
            || crate::rules::path_utils::has_codemod_snapshot_infix(path)
            || crate::rules::path_utils::has_test_suite_factory_suffix(path)
            || lower.contains("/dtslint/")
            || lower.starts_with("dtslint/")
            || lower.contains("/spec-dtslint/")
            || lower.starts_with("spec-dtslint/")
            || lower.contains("/__tests_dts__/")
            || lower.starts_with("__tests_dts__/")
            || lower.contains("/__tests__/")
            || lower.starts_with("__tests__/")
            || lower.contains("/fixtures/")
            || has_integration_fixture_app_shape(&lower)
            || lower.contains("/__mocks__/")
            || lower.contains("/mocks/")
            || lower.starts_with("mocks/")
            || lower.contains("/e2e/")
            || lower.starts_with("e2e/")
            || lower.contains(".test.")
            || lower.contains(".spec.")
            || lower.contains(".e2e.")
            || lower.contains(".cy.")
            || lower.contains("_test.")
            || lower.contains("_spec.")
            || lower.ends_with("/test.ts")
            || lower.ends_with("/test.tsx")
            || lower.ends_with("/test.js")
            || lower.ends_with("/test.jsx")
            || lower == "test.ts"
            || lower == "test.tsx"
            || lower == "test.js"
            || lower == "test.jsx",
        in_test_internal_dir: has_test_internal_dir(&lower),
        in_node_modules: lower.contains("/node_modules/"),
        in_storybook: lower.contains(".stories.") || has_storybook_segment(&lower),
        is_vendored: has_vendored_segment(&lower),
        is_relaxed_dir: has_relaxed_segment(&lower),
        in_aux_dir: crate::rules::path_utils::is_aux_dir_path(path),
        in_fuzz_targets: crate::rules::path_utils::is_fuzz_targets_path(path),
        in_benchmark_dir: is_benchmark_path(&lower),
        is_framework_hook_file: is_framework_hook_file(path),
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
        // e2e directory + `.e2e.` marker + `_test.` infix conventions.
        assert!(scan_path(&PathBuf::from("e2e/foo.spec.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/e2e/login.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/foo.e2e.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/foo_test.ts")).in_test_dir);
        // Jasmine/Angular underscore-spec convention (issue #1737).
        assert!(scan_path(&PathBuf::from("packages/schematics/recorder_spec.ts")).in_test_dir);
        // Test-helper infrastructure directories (issue #481).
        assert!(scan_path(&PathBuf::from("src/api/test-helpers/als-proxy.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/test-helper/db.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("test-helpers/setup.ts")).in_test_dir);
        // Visual-regression test directory convention (issue #1866).
        assert!(scan_path(&PathBuf::from("test-vr/charts.tsx")).in_test_dir);
        assert!(scan_path(&PathBuf::from("packages/recharts/test-vr/area.tsx")).in_test_dir);
        // tsd type-testing convention (issue #793).
        assert!(scan_path(&PathBuf::from("test-d/schema.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/test-d/types.ts")).in_test_dir);
        // tsd `test-tsd/` directory convention (issue #2338): a sibling helper
        // without the `.test-d.` infix is still type-test infrastructure.
        assert!(scan_path(&PathBuf::from("test-tsd/common.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("packages/foo/test-tsd/common.ts")).in_test_dir);
        // RSpec/Jasmine/Mocha `spec/` directory convention (issue #2306):
        // hyphen-suffixed `foo-spec.ts` files have no `.spec.` infix, so the
        // directory segment is what marks them as tests.
        assert!(scan_path(&PathBuf::from("spec/operators/foo-spec.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("packages/rxjs/spec/operators/first-spec.ts")).in_test_dir);
        // Segment match — `respec/`, a `myspec.ts` file, or a file literally
        // named `spec.ts` are not a `spec/` directory.
        assert!(!scan_path(&PathBuf::from("respec/x.ts")).in_test_dir);
        assert!(!scan_path(&PathBuf::from("myspec.ts")).in_test_dir);
        assert!(!scan_path(&PathBuf::from("src/spec.ts")).in_test_dir);
        // dtslint-style `spec-dtslint/` type-test directory (issue #2308): a
        // compound segment that is neither a bare `spec/` nor a bare `dtslint/`
        // dir, so it needs its own segment marker.
        assert!(scan_path(&PathBuf::from("spec-dtslint/index.d.ts")).in_test_dir);
        assert!(
            scan_path(&PathBuf::from(
                "packages/rxjs/spec-dtslint/operators/bufferTime-spec.ts"
            ))
            .in_test_dir
        );
        // Segment match — `spec-dtslint-helpers.ts` is a file, not the dir
        // segment, and an unrelated source file is not a test dir.
        assert!(!scan_path(&PathBuf::from("spec-dtslint-helpers.ts")).in_test_dir);
        assert!(!scan_path(&PathBuf::from("src/foo.ts")).in_test_dir);
        // dtslint type-testing convention (issue #1006).
        assert!(scan_path(&PathBuf::from("dtslint/Array.ts")).in_test_dir);
        // dtslint-style `__tests_dts__/` type-test directory (issue #1660).
        assert!(
            scan_path(&PathBuf::from("packages/vite/src/node/__tests_dts__/config.ts"))
                .in_test_dir
        );
        assert!(scan_path(&PathBuf::from("__tests_dts__/config.ts")).in_test_dir);
        // Segment match — a substring like `my__tests_dts__data/` is not a dir.
        assert!(!scan_path(&PathBuf::from("src/my__tests_dts__data/index.ts")).in_test_dir);
        // PascalCase `Tests`/`Spec` test-suite-factory convention (issue #1661).
        assert!(
            scan_path(&PathBuf::from(
                "packages/integration-testsuite/src/apolloServerTests.ts"
            ))
            .in_test_dir
        );
        assert!(scan_path(&PathBuf::from("src/httpServerSpec.tsx")).in_test_dir);
        // A production file ending in lowercase `tests` is not a test factory.
        assert!(!scan_path(&PathBuf::from("src/manifests.ts")).in_test_dir);
        // MSW + Jest mock infrastructure directories (issue #1883).
        assert!(scan_path(&PathBuf::from("src/mocks/db.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("mocks/handlers.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/__mocks__/server.ts")).in_test_dir);
        assert!(
            scan_path(&PathBuf::from(
                "very-well-written-projects/typescript/fp-ts/dtslint/Array.ts"
            ))
            .in_test_dir
        );
        // jscodeshift/babel codemod snapshot fixtures (issue #1353).
        assert!(scan_path(&PathBuf::from("foo.actual.js")).in_test_dir);
        assert!(scan_path(&PathBuf::from("bar.expected.ts")).in_test_dir);
        assert!(
            scan_path(&PathBuf::from(
                "src/deprecations/typography-props/test-cases/theme.actual.js"
            ))
            .in_test_dir
        );
        // Ordinary modules — including a leading-`actual` word — are not fixtures.
        assert!(!scan_path(&PathBuf::from("foo.js")).in_test_dir);
        assert!(!scan_path(&PathBuf::from("src/factual.ts")).in_test_dir);
        // Integration-test fixture-app convention `integration/<app>/src/`
        // (issue #2378): NestJS/Angular/TypeORM mini-apps run by sibling e2e
        // suites, never published.
        assert!(
            scan_path(&PathBuf::from(
                "integration/microservices/src/nats/nats.controller.ts"
            ))
            .in_test_dir
        );
        assert!(
            scan_path(&PathBuf::from("packages/foo/integration/graphql/src/cats/cats.resolver.ts"))
                .in_test_dir
        );
        // A production `src/integration/` module (no nested `src/` after the
        // `integration/` segment) is not the fixture-app shape.
        assert!(!scan_path(&PathBuf::from("src/integration/payment-gateway.ts")).in_test_dir);
    }

    #[test]
    fn test_internal_dir_convention_issue1150() {
        // `internal/` directly under a test root marks tests that deliberately
        // exercise internal implementation details (Azure-SDK convention).
        assert!(
            scan_path(&PathBuf::from(
                "sdk/core/core-rest-pipeline/test/internal/node/userAgent.spec.ts"
            ))
            .in_test_internal_dir
        );
        assert!(scan_path(&PathBuf::from("test/internal/foo.spec.ts")).in_test_internal_dir);
        assert!(scan_path(&PathBuf::from("tests/internal/foo.spec.ts")).in_test_internal_dir);
        assert!(scan_path(&PathBuf::from("__tests__/internal/foo.spec.ts")).in_test_internal_dir);
        assert!(scan_path(&PathBuf::from("_tests_/internal/foo.spec.ts")).in_test_internal_dir);
        // `test/public/` verifies the public API contract — not exempt.
        assert!(!scan_path(&PathBuf::from("test/public/foo.spec.ts")).in_test_internal_dir);
        // A plain `test/` file (no `internal/` child) is not exempt.
        assert!(!scan_path(&PathBuf::from("test/foo.spec.ts")).in_test_internal_dir);
        // An `internal/` not under a test root is not a test-internal dir.
        assert!(!scan_path(&PathBuf::from("src/internal/foo.ts")).in_test_internal_dir);
    }

    #[test]
    fn closes_cypress_cy_extension_gap_issue1868() {
        // Issue #1868: Cypress E2E specs use the `.cy.*` extension. They are
        // loaded by the Cypress runner, never imported as modules, so every
        // rule reading `ctx.file.path_segments.in_test_dir` must treat them as
        // tests.
        assert!(scan_path(&PathBuf::from("cypress/e2e/manualRegisterForm.cy.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/Login.cy.js")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/Login.cy.tsx")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/Login.cy.jsx")).in_test_dir);
        // A plain co-located source file is still not a test dir.
        assert!(!scan_path(&PathBuf::from("src/Login.tsx")).in_test_dir);
    }

    #[test]
    fn closes_test_d_filename_infix_gap_pr2144() {
        // PR #2144 gap: a `.test-d.` FILENAME infix with NO `/test-d/`
        // directory (the tsd co-located convention) must classify as a test
        // dir, so every rule reading `ctx.file.path_segments.in_test_dir`
        // treats it as a test for free.
        assert!(scan_path(&PathBuf::from("src/Component.test-d.tsx")).in_test_dir);
        assert!(scan_path(&PathBuf::from("schema.test-d.ts")).in_test_dir);
        // A plain co-located source file is still not a test dir.
        assert!(!scan_path(&PathBuf::from("src/Component.tsx")).in_test_dir);
    }

    #[test]
    fn closes_type_probe_filename_infix_gap_issue1915() {
        // Issue #1915: date-fns `.tp.` type-probe FILENAME infix (e.g.
        // `addBusinessDays/test.tp.ts`) must classify as a test dir so every
        // rule reading `ctx.file.path_segments.in_test_dir` treats it as a test.
        assert!(scan_path(&PathBuf::from("src/addBusinessDays/test.tp.ts")).in_test_dir);
        assert!(scan_path(&PathBuf::from("src/addDays/foo.tp.tsx")).in_test_dir);
        // A plain co-located source file is still not a test dir.
        assert!(!scan_path(&PathBuf::from("src/addDays/index.ts")).in_test_dir);
    }

    #[test]
    fn in_aux_dir_set_for_non_shipped_dirs() {
        for dir in [
            "scripts",
            "bin",
            "config",
            "migrations",
            "samples",
            "examples",
            "templates",
            "scaffold",
            "boilerplate",
        ] {
            assert!(
                scan_path(&PathBuf::from(format!("pkg/{dir}/file.ts"))).in_aux_dir,
                "{dir}/ should set in_aux_dir"
            );
        }
        assert!(!scan_path(&PathBuf::from("src/app.ts")).in_aux_dir);
    }

    #[test]
    fn in_fuzz_targets_set_for_cargo_fuzz_path() {
        assert!(scan_path(&PathBuf::from("fuzz/fuzz_targets/x.rs")).in_fuzz_targets);
        assert!(!scan_path(&PathBuf::from("src/lib.rs")).in_fuzz_targets);
    }

    #[test]
    fn in_benchmark_dir_set_for_benches_and_bench_markers_issue1497() {
        // `benches/` directory segment (Rust convention) and *_bench file stems.
        assert!(scan_path(&PathBuf::from("foo/benches/x.rs")).in_benchmark_dir);
        assert!(scan_path(&PathBuf::from("diesel_bench/benches/consts.rs")).in_benchmark_dir);
        assert!(scan_path(&PathBuf::from("src/parse_bench.rs")).in_benchmark_dir);
        assert!(scan_path(&PathBuf::from("src/parse-bench.rs")).in_benchmark_dir);
        assert!(scan_path(&PathBuf::from("src/parse.bench.ts")).in_benchmark_dir);
        // Plain production files are not benchmarks.
        assert!(!scan_path(&PathBuf::from("src/foo.rs")).in_benchmark_dir);
        // Exact-segment match only: `benches-old/` is not a benchmark dir.
        assert!(!scan_path(&PathBuf::from("benches-old/x.rs")).in_benchmark_dir);
    }

    #[test]
    fn file_ctx_in_benchmark_dir_method_issue1497() {
        let project = ProjectCtx::empty();
        let bench = FileCtx::build(
            Path::new("diesel_bench/benches/consts.rs"),
            "const X: &str = \"x\";",
            Language::Rust,
            &project,
        );
        assert!(bench.in_benchmark_dir());
        let prod = FileCtx::build(
            Path::new("src/foo.rs"),
            "const X: &str = \"x\";",
            Language::Rust,
            &project,
        );
        assert!(!prod.in_benchmark_dir());
    }

    #[test]
    fn framework_hook_files_set_flag_issue1154() {
        // Vitest globalSetup / setup and Playwright globalSetup / globalTeardown
        // hook files resolved by path — anonymous default export by convention.
        for name in [
            "setup.ts",
            "setup.mts",
            "global-setup.ts",
            "globalSetup.ts",
            "teardown.ts",
            "global-teardown.ts",
            "globalTeardown.ts",
        ] {
            assert!(
                scan_path(&PathBuf::from(format!("test/utils/{name}"))).is_framework_hook_file,
                "{name} should set is_framework_hook_file"
            );
        }
        // The stem must equal a hook name exactly — a longer identifier that
        // merely starts with `setup`/`teardown` is a regular module.
        assert!(!scan_path(&PathBuf::from("src/setupRouter.ts")).is_framework_hook_file);
        assert!(!scan_path(&PathBuf::from("src/teardownManager.ts")).is_framework_hook_file);
        assert!(!scan_path(&PathBuf::from("src/index.ts")).is_framework_hook_file);
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
    fn relaxed_dirs_cover_samples_and_docs() {
        // examples/benches/fixtures plus samples/ and docs/ (issue #1124).
        assert!(scan_path(&PathBuf::from("examples/foo.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("benches/foo.rs")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("fixtures/foo.ts")).is_relaxed_dir);
        // `__fixtures__/` test-fixture convention (issue #1154).
        assert!(scan_path(&PathBuf::from("src/__fixtures__/foo.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("a/b/__fixtures__/x.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("samples/foo.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("a/b/samples/x.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("docs/y.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("a/b/docs/y.ts")).is_relaxed_dir);
        assert!(!scan_path(&PathBuf::from("src/foo.ts")).is_relaxed_dir);
    }

    #[test]
    fn relaxed_dirs_cover_example_and_demo_segments() {
        // Singular example/ plus demo/ and demos/ demonstration dirs (issue #1918).
        assert!(scan_path(&PathBuf::from("example/foo.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("a/b/example/x.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("demo/foo.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("a/b/demo/x.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("demos/foo.ts")).is_relaxed_dir);
        assert!(scan_path(&PathBuf::from("a/b/demos/x.ts")).is_relaxed_dir);
        // date-fns case: nested examples/ subdirectory.
        assert!(
            scan_path(&PathBuf::from("pkgs/core/examples/node-esm/constants.js")).is_relaxed_dir
        );
    }

    #[test]
    fn relaxed_dirs_do_not_match_substrings() {
        // Exact-segment match only: `example-app/` and `demonstration/` are not
        // relaxed dirs.
        assert!(!scan_path(&PathBuf::from("src/example-app/foo.ts")).is_relaxed_dir);
        assert!(!scan_path(&PathBuf::from("src/demonstration/foo.ts")).is_relaxed_dir);
        assert!(!scan_path(&PathBuf::from("src/documentation/foo.ts")).is_relaxed_dir);
    }

    #[test]
    fn storybook_filename_and_dirs() {
        // `.stories.` filename convention.
        assert!(scan_path(&PathBuf::from("src/Button.stories.tsx")).in_storybook);
        // Helper/story files inside a `stories/` directory without a
        // `.stories.` name (issue #1982).
        assert!(
            scan_path(&PathBuf::from("storybook/stories/internal/KeyLogger.tsx")).in_storybook
        );
        assert!(scan_path(&PathBuf::from("src/stories/Header.tsx")).in_storybook);
        // A top-level `storybook/` package directory.
        assert!(scan_path(&PathBuf::from("storybook/preview.tsx")).in_storybook);
    }

    #[test]
    fn storybook_does_not_match_substrings() {
        assert!(!scan_path(&PathBuf::from("src/mystories.ts")).in_storybook);
        assert!(!scan_path(&PathBuf::from("src/storybook-static/index.js")).in_storybook);
        assert!(!scan_path(&PathBuf::from("src/index.ts")).in_storybook);
    }

    #[test]
    fn gen_filename_suffixes_are_generated() {
        // TanStack Router route trees (issue #1010).
        assert!(is_generated_filename(&PathBuf::from(
            "examples/vue/basic-file-based-jsx/src/routeTree.gen.ts"
        )));
        assert!(is_generated_filename(&PathBuf::from("src/api/foo.generated.ts")));
    }

    #[test]
    fn non_gen_filenames_are_not_generated() {
        assert!(!is_generated_filename(&PathBuf::from("src/mygen.ts")));
        assert!(!is_generated_filename(&PathBuf::from("src/generated.ts")));
        assert!(!is_generated_filename(&PathBuf::from("src/app.ts")));
    }

    #[test]
    fn tanstack_router_header_is_generated() {
        let src = "/* eslint-disable */\n// @ts-nocheck\n// This file was automatically generated by TanStack Router.\n";
        assert!(is_generated_content(src));
    }

    #[test]
    fn bare_eslint_disable_header_is_generated() {
        assert!(is_generated_content("/* eslint-disable */\n"));
    }

    #[test]
    fn targeted_eslint_disable_is_not_generated() {
        assert!(!is_generated_content("/* eslint-disable no-console */\nconst x = 1;\n"));
    }

    #[test]
    fn protobuf_eslint_disable_header_is_generated_issue1144() {
        // Issue #1144: the Protobuf compiler emits a targeted eslint-disable
        // header listing `no-prototype-builtins`; that rule list is a codegen
        // signature.
        let src = "/*eslint-disable block-scoped-var, id-length, no-control-regex, no-magic-numbers, no-prototype-builtins, no-redeclare, no-shadow, no-var, sort-vars*/\nimport * as $protobuf from \"protobufjs/minimal\";\n";
        assert!(is_generated_content(src));
    }

    #[test]
    fn unrelated_targeted_eslint_disable_is_not_generated_issue1144() {
        // A hand-written targeted disable without `no-prototype-builtins` stays
        // non-generated.
        assert!(!is_generated_content("/* eslint-disable no-console, no-shadow */\nconst x = 1;\n"));
    }

    #[test]
    fn autorest_self_declaration_header_is_generated_issue1135() {
        // Issue #1135: AutoRest/TypeSpec model files declare themselves as
        // generated without a `@generated` / `do not edit` marker.
        let src = "// Copyright (c) Microsoft Corporation.\n// Licensed under the MIT License.\n\n/**\n * This file contains only generated model types and their (de)serializers.\n * Disable the following rules for internal models with '_' prefix.\n */\n/* eslint-disable @typescript-eslint/naming-convention */\nexport interface LinkedResource { uniqueName: string; id: string; }\n";
        assert!(is_generated_content(src));
    }

    #[test]
    fn incidental_generated_mention_is_not_generated_issue1135() {
        // A hand-written comment that merely mentions the word "generated"
        // (not a whole-file self-declaration) must stay non-generated.
        assert!(!is_generated_content(
            "// The token below was generated manually for local testing.\nexport const token = \"abc\";\n"
        ));
    }

    #[test]
    fn generated_dir_segment_is_generated_issue1144() {
        assert!(is_in_generated_dir(&PathBuf::from("src/generated/clientProto.js")));
        assert!(is_in_generated_dir(&PathBuf::from(
            "sdk/web-pubsub/web-pubsub-client-protobuf/src/generated/clientProto.js"
        )));
    }

    #[test]
    fn generated_dir_does_not_match_substrings_issue1144() {
        assert!(!is_in_generated_dir(&PathBuf::from("src/generated-utils/foo.ts")));
        assert!(!is_in_generated_dir(&PathBuf::from("src/regenerated/foo.ts")));
        assert!(!is_in_generated_dir(&PathBuf::from("src/app.ts")));
    }

    #[test]
    fn plain_source_is_not_generated() {
        assert!(!is_generated_content("const x = 1;\n"));
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
