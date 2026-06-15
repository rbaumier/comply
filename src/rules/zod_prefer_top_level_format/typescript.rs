//! zod-prefer-top-level-format backend — flag `z.string().email()`,
//! `z.string().url()`, `z.string().uuid()`, `z.number().int()`.
//!
//! Why: Zod v4 exposes top-level format functions (`z.email()`, `z.url()`,
//! `z.uuid()`, `z.int()`, `z.iso.datetime()`) that are shorter, faster,
//! and tree-shakeable compared to the `.string().method()` chain.
//!
//! These helpers exist only in zod v4, so the suggestion fires only when the
//! nearest `package.json` proves zod resolves to v4+ (any dep section, falling
//! back to the manifest's own `version` when it is the zod package itself).
//! zod's own backward-compat sources are never flagged: files under a `/v3/`
//! path segment, and files in the `classic/` re-export layer of the zod package
//! itself, where the chained API is the one being maintained.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::PackageJson;
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["call_expression"];

const STRING_CHAIN_METHODS: &[(&str, &str)] = &[
    ("email", "z.email()"),
    ("url", "z.url()"),
    ("uuid", "z.uuid()"),
    ("cuid", "z.cuid()"),
    ("ulid", "z.ulid()"),
    ("datetime", "z.iso.datetime()"),
    ("date", "z.iso.date()"),
    ("time", "z.iso.time()"),
    ("ip", "z.ipv4() or z.ipv6()"),
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Top-level format helpers (`z.email()` etc.) only exist in zod v4.
        // Skip zod's own backward-compat sources, where the chained API is the
        // one being maintained, not a consumer choosing an inferior form.
        if path_has_v3_segment(ctx.path) || is_zod_classic_compat_source(ctx) {
            return;
        }
        // Only fire when the nearest package.json proves zod resolves to v4+.
        if !zod_is_v4_or_later(ctx) {
            return;
        }

        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "member_expression" {
            return;
        }
        let Some(property) = function.child_by_field_name("property") else {
            return;
        };
        let Some(object) = function.child_by_field_name("object") else {
            return;
        };
        let Ok(method_name) = property.utf8_text(source_bytes) else {
            return;
        };

        // Check z.string().<method>()
        if let Some((_, replacement)) = STRING_CHAIN_METHODS.iter().find(|(m, _)| *m == method_name)
            && is_z_string_call(object, source_bytes)
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "zod-prefer-top-level-format".into(),
                message: format!(
                    "`z.string().{method_name}()` — use `{replacement}` \
                     directly. Top-level format helpers are shorter, \
                     faster, and tree-shakeable."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        // Check z.number().int()
        if method_name == "int" && is_z_number_call(object, source_bytes) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "zod-prefer-top-level-format".into(),
                message: "`z.number().int()` — use `z.int()` directly.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn is_z_string_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    is_z_method_call(node, "string", source)
}

fn is_z_number_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    is_z_method_call(node, "number", source)
}

fn is_z_method_call(node: tree_sitter::Node, method: &str, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    function
        .utf8_text(source)
        .is_ok_and(|t| t == format!("z.{method}"))
}

/// True when the file path contains a `/v3/` segment — zod ships its v3-compat
/// API under `packages/zod/src/v3/`, where the chained `.email()` form is the
/// correct one and must not be flagged.
fn path_has_v3_segment(path: &std::path::Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_str() == Some("v3"))
}

/// True for a file inside zod's own `classic/` backward-compat layer — the
/// directory where zod re-exports the chained API so v3 users can upgrade. Such
/// files (e.g. `v4/classic/from-json-schema.ts`) maintain the chained form
/// deliberately; `z.number().int()` produces a `ZodNumber`, distinct from
/// `z.int()`'s `ZodInt`, so the suggested replacement is not type-equivalent.
///
/// Gated on the nearest manifest being the zod package itself (`name == "zod"`)
/// so a consumer's unrelated `classic/` directory is still nudged.
fn is_zod_classic_compat_source(ctx: &CheckCtx) -> bool {
    let in_classic_dir = ctx
        .path
        .components()
        .any(|c| c.as_os_str().to_str() == Some("classic"));
    if !in_classic_dir {
        return false;
    }
    ctx.project
        .nearest_package_json(ctx.path)
        .is_some_and(|pkg| pkg.name.as_deref() == Some("zod"))
}

/// True when the nearest `package.json` proves zod resolves to v4 or later.
///
/// Looks across every dependency section, and — because the zod package itself
/// does not list `zod` as a dependency — falls back to the manifest's own
/// top-level `version` when it is the zod package (`name == "zod"`). When the
/// version cannot be proven >= 4 (no manifest, undeclared, or a range whose
/// smallest major is < 4 such as `^3 || ^4`), this returns `false`.
fn zod_is_v4_or_later(ctx: &CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    zod_version_range(&pkg)
        .and_then(range_min_major)
        .is_some_and(|major| major >= 4)
}

/// The declared zod version range from the nearest manifest: a dependency entry
/// in any section, or the manifest's own `version` when it is the zod package.
fn zod_version_range(pkg: &PackageJson) -> Option<&str> {
    pkg.dependencies
        .get("zod")
        .or_else(|| pkg.dev_dependencies.get("zod"))
        .or_else(|| pkg.peer_dependencies.get("zod"))
        .or_else(|| pkg.optional_dependencies.get("zod"))
        .map(String::as_str)
        .or_else(|| {
            (pkg.name.as_deref() == Some("zod")).then(|| pkg.version.as_deref())?
        })
}

/// Smallest major version a range can resolve to. Splits on `||`, takes the
/// first numeric run of each alternative as its major, and returns the minimum
/// across alternatives. Returns `None` when no alternative contains a number,
/// so undeterminable ranges (e.g. `latest`, `*`, a workspace/git spec) do not
/// fire. `^3 || ^4` yields `Some(3)`, keeping v3-compatible projects silent.
fn range_min_major(range: &str) -> Option<u32> {
    range
        .split("||")
        .filter_map(first_numeric_run)
        .min()
}

/// First contiguous run of ASCII digits in `s`, parsed as a `u32`. Skips any
/// leading non-digit prefix (`^`, `~`, `>=`, `v`, whitespace).
fn first_numeric_run(s: &str) -> Option<u32> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let end = s[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(s.len(), |offset| start + offset);
    s[start..end].parse().ok()
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
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    /// Run the rule against `source` written to `rel_path` inside a temp project
    /// whose `package.json` is `pkg_json` (or no manifest when `None`).
    fn run_in_project(pkg_json: Option<&str>, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        if let Some(pkg) = pkg_json {
            fs::write(dir.path().join("package.json"), pkg).unwrap();
        }
        let src_path = dir.path().join(rel_path);
        fs::create_dir_all(src_path.parent().unwrap()).unwrap();
        fs::write(&src_path, source).unwrap();
        let src_path = fs::canonicalize(&src_path).unwrap();

        let source_file = SourceFile {
            path: src_path.clone(),
            language: Language::TypeScript,
        };
        let refs: Vec<&SourceFile> = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        // Build the real per-file context and honour the production gate so the
        // `skip_in_test_dir` exemption is exercised, not bypassed.
        let file = crate::rules::file_ctx::FileCtx::build(
            &src_path,
            source,
            Language::TypeScript,
            &project,
        );
        if !super::super::META.applies_to_file(&file) {
            return vec![];
        }

        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, &src_path, &project, &file)
    }

    const V4_PKG: &str = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^4.0.0"}}"#;

    #[test]
    fn flags_string_email_on_v4() {
        let d = run_in_project(Some(V4_PKG), "app.ts", "const s = z.string().email();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("z.email()"));
    }

    #[test]
    fn flags_string_url_on_v4() {
        assert_eq!(
            run_in_project(Some(V4_PKG), "app.ts", "const s = z.string().url();").len(),
            1
        );
    }

    #[test]
    fn flags_number_int_on_v4() {
        assert_eq!(
            run_in_project(Some(V4_PKG), "app.ts", "const s = z.number().int();").len(),
            1
        );
    }

    #[test]
    fn allows_top_level_format_on_v4() {
        assert!(run_in_project(Some(V4_PKG), "app.ts", "const s = z.email();").is_empty());
        assert!(run_in_project(Some(V4_PKG), "app.ts", "const s = z.int();").is_empty());
    }

    #[test]
    fn allows_plain_string_schema_on_v4() {
        assert!(run_in_project(Some(V4_PKG), "app.ts", "const s = z.string();").is_empty());
    }

    #[test]
    fn silent_on_v3_project() {
        let pkg = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^3"}}"#;
        assert!(run_in_project(Some(pkg), "app.ts", "const s = z.string().email();").is_empty());
    }

    #[test]
    fn silent_on_v3_or_v4_range() {
        let pkg = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"^3 || ^4"}}"#;
        assert!(run_in_project(Some(pkg), "app.ts", "const s = z.string().email();").is_empty());
    }

    #[test]
    fn silent_without_package_json() {
        assert!(run_in_project(None, "app.ts", "const s = z.string().email();").is_empty());
    }

    #[test]
    fn silent_when_zod_undeclared() {
        let pkg = r#"{"name":"app","version":"1.0.0","dependencies":{"react":"^18"}}"#;
        assert!(run_in_project(Some(pkg), "app.ts", "const s = z.string().email();").is_empty());
    }

    #[test]
    fn silent_on_undeterminable_version() {
        let pkg = r#"{"name":"app","version":"1.0.0","dependencies":{"zod":"latest"}}"#;
        assert!(run_in_project(Some(pkg), "app.ts", "const s = z.string().email();").is_empty());
    }

    #[test]
    fn fires_via_dev_dependencies() {
        let pkg = r#"{"name":"app","version":"1.0.0","devDependencies":{"zod":"4.1.0"}}"#;
        assert_eq!(
            run_in_project(Some(pkg), "app.ts", "const s = z.string().email();").len(),
            1
        );
    }

    #[test]
    fn fires_via_own_version_when_zod_package() {
        let pkg = r#"{"name":"zod","version":"4.0.5"}"#;
        assert_eq!(
            run_in_project(Some(pkg), "src/index.ts", "const s = z.string().email();").len(),
            1
        );
    }

    #[test]
    fn silent_on_v3_path_segment_inside_v4_package() {
        let pkg = r#"{"name":"zod","version":"4.0.5"}"#;
        assert!(
            run_in_project(Some(pkg), "src/v3/types.ts", "const s = z.string().email();")
                .is_empty()
        );
    }

    const ZOD_PKG: &str = r#"{"name":"zod","version":"4.0.5"}"#;

    // Issue #3369, context 1: zod's v4/classic backward-compat tests
    // intentionally exercise the chained API to verify it still works. The
    // central `skip_in_test_dir` gate (`/tests/` + `.test.` path) exempts them.
    #[test]
    fn silent_in_classic_compat_tests_issue3369() {
        assert!(
            run_in_project(
                Some(ZOD_PKG),
                "packages/zod/src/v4/classic/tests/string.test.ts",
                r#"const uuid = z.string().uuid("custom error");"#,
            )
            .is_empty()
        );
        assert!(
            run_in_project(
                Some(ZOD_PKG),
                "packages/zod/src/v4/classic/tests/template-literal.test.ts",
                r#"const email = z.templateLiteral(["", z.string().email()]);"#,
            )
            .is_empty()
        );
    }

    // Issue #3369, context 2: the zod package's own `classic/` compat-layer
    // source maintains the chained form deliberately (`z.number().int()` is a
    // `ZodNumber`, distinct from `z.int()`'s `ZodInt`); it is not a consumer to
    // nudge.
    #[test]
    fn silent_in_classic_compat_source_issue3369() {
        assert!(
            run_in_project(
                Some(ZOD_PKG),
                "packages/zod/src/v4/classic/from-json-schema.ts",
                "const numberSchema = z.number().int();",
            )
            .is_empty()
        );
    }

    // Over-exemption guard: a normal application file that happens to live under
    // a `classic/` directory is still nudged — the exemption is gated on the
    // manifest being the zod package itself.
    #[test]
    fn nudges_classic_dir_in_consumer_app() {
        assert_eq!(
            run_in_project(
                Some(V4_PKG),
                "src/classic/schema.ts",
                "const s = z.string().email();",
            )
            .len(),
            1
        );
    }

    #[test]
    fn range_min_major_takes_smallest_alternative() {
        assert_eq!(range_min_major("^3 || ^4"), Some(3));
        assert_eq!(range_min_major("^4 || ^3"), Some(3));
        assert_eq!(range_min_major(">=4.0.0"), Some(4));
        assert_eq!(range_min_major("4.1.0"), Some(4));
        assert_eq!(range_min_major("latest"), None);
        assert_eq!(range_min_major("*"), None);
    }
}
