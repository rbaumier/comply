//! zod-prefer-top-level-format backend — flag `z.string().email()`,
//! `z.string().url()`, `z.string().uuid()`, `z.number().int()`.
//!
//! Why: Zod v4 exposes top-level format functions (`z.email()`, `z.url()`,
//! `z.uuid()`, `z.int()`, `z.iso.datetime()`) that are shorter, faster,
//! and tree-shakeable compared to the `.string().method()` chain.
//!
//! These helpers only exist in Zod v4 — in v3, `z.string().email()` is the
//! only API and `z.email()` is a runtime error. The check therefore fires
//! only when the nearest `package.json` declares a zod version that allows
//! v4+, and never inside a `/v3/` legacy subtree.

use crate::diagnostic::{Diagnostic, Severity};
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
        if !project_uses_zod_v4(ctx) {
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

/// True when the suggested top-level helpers (`z.email()` etc.) actually exist
/// for the file being linted: the project resolves zod v4+ and the file is not
/// part of a `/v3/` legacy subtree. Both conditions must hold — a v4 package
/// still ships its v3-compat sources under `src/v3/`, where the chained API is
/// correct.
fn project_uses_zod_v4(ctx: &CheckCtx) -> bool {
    if path_is_v3_subtree(ctx.path) {
        return false;
    }
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    declared_zod_min_major(&pkg).is_some_and(|major| major >= 4)
}

/// Minimum zod major version the project resolves to, or `None` when zod's
/// version cannot be determined. The zod package itself names no `zod`
/// dependency, so its own `version` field is consulted when the nearest
/// manifest *is* zod.
fn declared_zod_min_major(pkg: &crate::project::PackageJson) -> Option<u32> {
    let spec = pkg
        .dependencies
        .get("zod")
        .or_else(|| pkg.dev_dependencies.get("zod"))
        .or_else(|| pkg.peer_dependencies.get("zod"))
        .or_else(|| pkg.optional_dependencies.get("zod"))
        .or_else(|| {
            (pkg.name.as_deref() == Some("zod"))
                .then(|| pkg.version.as_ref())
                .flatten()
        })?;
    range_min_major(spec)
}

/// Minimum major version a semver range string allows. For a range with `||`
/// alternatives (e.g. `^3 || ^4`) the smallest alternative wins — if v3 is
/// permitted, the v4-only helpers are not guaranteed to exist, so the check
/// must not fire.
fn range_min_major(spec: &str) -> Option<u32> {
    spec.split("||").filter_map(alternative_min_major).min()
}

/// First numeric run in a single range alternative, read as a major version.
/// Handles `4`, `^4`, `~4.1`, `>=4.0.0`, `4.x`, `4.2.1`.
fn alternative_min_major(alt: &str) -> Option<u32> {
    let bytes = alt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            return std::str::from_utf8(&bytes[start..i]).ok()?.parse().ok();
        }
        i += 1;
    }
    None
}

/// True when any path segment is exactly `v3` — zod ships its v3-compat sources
/// under `src/v3/`, where `z.string().email()` is the correct (and only) API.
fn path_is_v3_subtree(path: &std::path::Path) -> bool {
    path.components().any(|c| c.as_os_str() == "v3")
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

    /// Run the rule against `source` placed at `rel_path` inside a temp project
    /// whose `package.json` is `pkg_json`. Mirrors production: `nearest_package_json`
    /// walks disk from the file path up to the manifest.
    fn run_in_project(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
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

        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &src_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    const V4_PKG: &str = r#"{"name":"app","version":"0.0.0","dependencies":{"zod":"^4.0.0"}}"#;
    const V3_PKG: &str = r#"{"name":"app","version":"0.0.0","dependencies":{"zod":"^3.23.0"}}"#;

    fn run_v4(source: &str) -> Vec<Diagnostic> {
        run_in_project(V4_PKG, "app.ts", source)
    }

    #[test]
    fn flags_string_email_on_v4() {
        assert_eq!(run_v4("const s = z.string().email();").len(), 1);
    }

    #[test]
    fn flags_string_url_on_v4() {
        assert_eq!(run_v4("const s = z.string().url();").len(), 1);
    }

    #[test]
    fn flags_number_int_on_v4() {
        assert_eq!(run_v4("const s = z.number().int();").len(), 1);
    }

    #[test]
    fn allows_top_level_format_on_v4() {
        assert!(run_v4("const s = z.email();").is_empty());
        assert!(run_v4("const s = z.int();").is_empty());
    }

    #[test]
    fn allows_plain_string_schema_on_v4() {
        assert!(run_v4("const s = z.string();").is_empty());
    }

    // ── #2071: do not suggest v4-only helpers on zod v3 source ──────────

    #[test]
    fn skips_string_email_on_v3_dependency() {
        let d = run_in_project(V3_PKG, "app.ts", "const s = z.string().email();");
        assert!(d.is_empty(), "z.email() does not exist in zod v3");
    }

    #[test]
    fn skips_string_url_on_v3_dependency() {
        let d = run_in_project(V3_PKG, "app.ts", "const s = z.string().url();");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_when_range_allows_v3() {
        // `^3 || ^4` permits a v3 install — the helper is not guaranteed to exist.
        let pkg = r#"{"name":"app","version":"0.0.0","dependencies":{"zod":"^3 || ^4"}}"#;
        let d = run_in_project(pkg, "app.ts", "const s = z.string().email();");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_when_zod_undeclared() {
        let pkg = r#"{"name":"app","version":"0.0.0"}"#;
        let d = run_in_project(pkg, "app.ts", "const s = z.string().email();");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_v3_legacy_subtree_in_v4_package() {
        // zod's own v4 package ships v3-compat sources under src/v3/.
        let pkg = r#"{"name":"zod","version":"4.4.3"}"#;
        let d = run_in_project(pkg, "src/v3/tests/string.test.ts", "const s = z.string().email();");
        assert!(d.is_empty(), "src/v3 subtree is the v3 API surface");
    }

    #[test]
    fn flags_in_zod_package_outside_v3() {
        // zod names no `zod` dependency; its own version field decides.
        let pkg = r#"{"name":"zod","version":"4.4.3"}"#;
        let d = run_in_project(pkg, "src/v4/classic/email.ts", "const s = z.string().email();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn range_min_major_takes_smallest_alternative() {
        assert_eq!(range_min_major("^4.0.0"), Some(4));
        assert_eq!(range_min_major(">=4"), Some(4));
        assert_eq!(range_min_major("4.x"), Some(4));
        assert_eq!(range_min_major("^3.23.0"), Some(3));
        assert_eq!(range_min_major("^3 || ^4"), Some(3));
        assert_eq!(range_min_major(""), None);
    }
}
