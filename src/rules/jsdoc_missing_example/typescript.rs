//! jsdoc-missing-example backend — every JSDoc on an exported function must
//! contain an `@example` tag.
//!
//! Walks `export_statement` nodes wrapping a `function_declaration`. If the
//! preceding sibling is a JSDoc comment (`/** ... */`) and that comment does
//! NOT contain `@example`, we flag it. Exports without ANY JSDoc are not
//! flagged here — that's `jsdoc-on-exported`'s job. The two rules compose:
//! one ensures presence, the other ensures completeness.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let public_patterns =
            ctx.config
                .string_list("jsdoc-missing-example", "public_patterns", ctx.lang);
        if !public_patterns.is_empty() && !path_matches_any(ctx.path, &public_patterns) {
            return Vec::new();
        }

        let source = ctx.source.as_bytes();
        let root = tree.root_node();
        let mut diagnostics = Vec::new();
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if child.kind() != "export_statement" {
                continue;
            }
            if !is_exported_function(child) {
                continue;
            }
            let Some(jsdoc_text) = jsdoc_text_above(child, source) else {
                // No JSDoc at all — that's jsdoc-on-exported's responsibility,
                // not ours. We only fire when a doc exists but lacks @example.
                continue;
            };
            if jsdoc_text.contains("@example") {
                continue;
            }
            let name = extract_exported_name(child, source).unwrap_or("<anonymous>");
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "jsdoc-missing-example".into(),
                message: format!(
                    "JSDoc on `{name}` is missing `@example`. Add a real call \
                     and its return value — examples are the fastest way for \
                     callers to understand the API."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

fn path_matches_any(path: &std::path::Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    patterns.iter().any(|pat| {
        globset::Glob::new(pat)
            .ok()
            .map(|g| g.compile_matcher().is_match(path_str.as_ref()))
            .unwrap_or(false)
    })
}

fn is_exported_function(export: tree_sitter::Node) -> bool {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            return true;
        }
    }
    false
}

fn jsdoc_text_above<'a>(export: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let prev = export.prev_named_sibling()?;
    if prev.kind() != "comment" {
        return None;
    }
    let text = prev.utf8_text(source).ok()?;
    if !text.starts_with("/**") {
        return None;
    }
    Some(text)
}

fn extract_exported_name<'a>(export: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = export.walk();
    for child in export.children(&mut cursor) {
        if child.kind() != "function_declaration" {
            continue;
        }
        return child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    fn run_with_public_patterns(source: &str, fake_path: &str, patterns: &[&str]) -> Vec<Diagnostic> {
        let tmp = TempDir::new().expect("tempdir");
        let patterns_toml = patterns
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            tmp.path().join("comply.toml"),
            format!("[rules.jsdoc-missing-example]\npublic_patterns = [{patterns_toml}]\n"),
        )
        .expect("write cfg");
        let cfg = crate::config::Config::load_from(tmp.path()).expect("load cfg");

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let ctx = crate::rules::backend::CheckCtx {
            path: Path::new(fake_path),
            path_arc: std::sync::Arc::from(Path::new(fake_path)),
            source,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: crate::files::Language::TypeScript,
        };
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_jsdoc_without_example() {
        let source = "/** Does foo. */\nexport function foo() {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_jsdoc_with_example() {
        let source = "/** Does foo.\n * @example\n *   foo();\n */\nexport function foo() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_export_without_jsdoc() {
        // No JSDoc at all — jsdoc-on-exported's job, not ours.
        assert!(run_on("export function foo() {}").is_empty());
    }

    #[test]
    fn ignores_non_exported_function() {
        let source = "/** Helper. */\nfunction helper() {}";
        assert!(run_on(source).is_empty());
    }

    // --- regression tests for issue #461 ---

    #[test]
    fn no_fp_on_internal_file_when_public_patterns_set() {
        // Internal utilities (e.g. authorization.ts, policy.ts) must not fire
        // when public_patterns is configured and the file doesn't match.
        let source = "/** Converts a raw role string to a MemberRole enum value. */\nexport function toMemberRole(role: string) { return role; }";
        let diags = run_with_public_patterns(source, "src/shared/authorization.ts", &["src/public/**"]);
        assert!(
            diags.is_empty(),
            "internal file should not fire when public_patterns restricts the rule: {diags:?}"
        );
    }

    #[test]
    fn fires_on_matching_public_file_when_public_patterns_set() {
        let source = "/** Does foo. */\nexport function foo() {}";
        let diags = run_with_public_patterns(source, "src/public/api.ts", &["src/public/**"]);
        assert_eq!(
            diags.len(),
            1,
            "public file should still fire when it matches public_patterns"
        );
    }

    #[test]
    fn empty_public_patterns_fires_everywhere() {
        // Default empty list: current behaviour is preserved — fires on all files.
        let source = "/** Does foo. */\nexport function foo() {}";
        let diags = run_with_public_patterns(source, "src/shared/authorization.ts", &[]);
        assert_eq!(
            diags.len(),
            1,
            "empty public_patterns should fire everywhere (backward-compatible)"
        );
    }
}
