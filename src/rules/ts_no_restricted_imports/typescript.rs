//! ts-no-restricted-imports backend — flag imports whose module specifier
//! matches a user-configured restriction pattern.
//!
//! The rule is opt-in via `comply.toml`:
//!
//! ```toml
//! [rules.ts-no-restricted-imports]
//! patterns = ["@banned/*", "lodash", "../internal/*"]
//! ```
//!
//! When the `patterns` list is absent or empty, the check returns
//! without scanning — no advisory noise on projects that never
//! configured a restriction list.
//!
//! Pattern matching is intentionally minimal (YAGNI): exact match, or
//! a trailing `*` wildcard that matches any suffix. Anything fancier
//! (brace groups, mid-string wildcards) belongs in a follow-up only
//! when a real use case appears.
//!
//! Both value imports (`import X from 'y'`) and type-only imports
//! (`import type X from 'y'`) are checked — the original ESLint rule
//! covers both and ignoring type imports would silently let restricted
//! modules sneak back in via `import type`.

use crate::diagnostic::{Diagnostic, Severity};

/// Return true if `specifier` matches `pattern`. Supports:
///   - exact match: `lodash` matches `lodash`
///   - trailing `*`: `@banned/*` matches `@banned/foo`, `@banned/a/b`
fn specifier_matches(specifier: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        specifier.starts_with(prefix)
    } else {
        specifier == pattern
    }
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let patterns = ctx
        .config
        .string_list("ts-no-restricted-imports", "patterns");
    if patterns.is_empty() {
        return;
    }

    let Some(source_node) = node.child_by_field_name("source") else {
        return;
    };
    let Ok(raw) = source_node.utf8_text(source) else {
        return;
    };
    let module_path = raw.trim_matches(|c| c == '\'' || c == '"');
    if module_path.is_empty() {
        return;
    }

    let Some(matched) = patterns
        .iter()
        .find(|p| specifier_matches(module_path, p))
    else {
        return;
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-restricted-imports".into(),
        message: format!(
            "Import from `{module_path}` matches restricted pattern `{matched}`. See comply.toml for the restriction list."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    /// Build a config that sets `patterns = [...]` for the rule, then
    /// run the check manually so we exercise the real config-reading
    /// path. Returns diagnostics for `source` parsed as TypeScript.
    fn run_with_patterns(source: &str, patterns: &[&str]) -> Vec<Diagnostic> {
        let tmp = TempDir::new().expect("tempdir");
        let cfg_path = tmp.path().join("comply.toml");
        let patterns_toml = patterns
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            &cfg_path,
            format!(
                "[rules.ts-no-restricted-imports]\npatterns = [{patterns_toml}]\n"
            ),
        )
        .expect("write cfg");
        let cfg = Config::load_from(tmp.path()).expect("load cfg");

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let ctx = CheckCtx {
            path: Path::new("t.ts"),
            source,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
        };
        Check.check(&ctx, &tree)
    }

    #[test]
    fn allows_any_import_when_no_restrictions_configured() {
        // Default config has no patterns set for this rule.
        let d = run_on("import type { Foo } from '@tanstack/react-table';");
        assert!(d.is_empty());
        let d = run_on("import { Foo } from 'bar';");
        assert!(d.is_empty());
        let d = run_on("import type Foo from './types';");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_matching_restricted_import() {
        let d = run_with_patterns(
            "import { foo } from '@banned/foo';",
            &["@banned/*"],
        );
        assert_eq!(d.len(), 1, "expected one diagnostic, got {d:?}");
        assert!(
            d[0].message.contains("@banned/foo")
                && d[0].message.contains("@banned/*"),
            "message should cite specifier and pattern: {}",
            d[0].message
        );
    }

    #[test]
    fn flags_matching_type_only_import() {
        // Type-only imports are included — the ESLint base rule covers
        // them and excluding them would leave a hole.
        let d = run_with_patterns(
            "import type { Foo } from 'legacy';",
            &["legacy"],
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ignores_non_matching_import() {
        let d = run_with_patterns(
            "import { ok } from '@ok/foo';",
            &["@banned/*"],
        );
        assert!(d.is_empty());
    }

    #[test]
    fn exact_pattern_does_not_match_prefix() {
        // `lodash` should not match `lodash/fp` — only trailing-`*`
        // patterns match prefixes.
        let d = run_with_patterns(
            "import fp from 'lodash/fp';",
            &["lodash"],
        );
        assert!(d.is_empty());
    }
}
