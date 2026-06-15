//! package-json-required-scripts backend — report when a `package.json` is
//! missing scripts the project configured as required.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use serde_json::Value;

#[derive(Debug)]
pub struct Check;

/// Script names present in the `scripts` object, or `None` when `scripts` is
/// absent or not an object.
fn declared_scripts(json: &Value) -> Option<Vec<&str>> {
    let scripts = json.get("scripts")?.as_object()?;
    Some(scripts.keys().map(String::as_str).collect())
}

fn find_scripts_line(source: &str) -> usize {
    for (i, line) in source.lines().enumerate() {
        if line.contains("\"scripts\"") {
            return i + 1;
        }
    }
    1
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.file_name().and_then(|f| f.to_str()) != Some("package.json") {
            return Vec::new();
        }

        let required = ctx.config.string_list(super::META.id, "scripts", ctx.lang);
        if required.is_empty() {
            return Vec::new();
        }

        let Ok(json) = serde_json::from_str::<Value>(ctx.source) else {
            return Vec::new();
        };

        let declared = declared_scripts(&json).unwrap_or_default();
        let missing: Vec<&str> = required
            .iter()
            .map(String::as_str)
            .filter(|name| !declared.contains(name))
            .collect();

        if missing.is_empty() {
            return Vec::new();
        }

        let message = if missing.len() == 1 {
            format!(
                "The required script {} is missing from package.json.",
                missing[0]
            )
        } else {
            format!(
                "The required scripts {} are missing from package.json.",
                missing.join(", ")
            )
        };

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: find_scripts_line(ctx.source),
            column: 1,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Run under the default config — no `scripts` configured.
    fn run_default(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("package.json"), source))
    }

    /// Build a config that sets `scripts = [...]` for the rule, then run the
    /// check against it so we exercise the real config-reading path.
    fn run_with_scripts(source: &str, scripts: &[&str]) -> Vec<Diagnostic> {
        let tmp = TempDir::new().expect("tempdir");
        let list = scripts
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            tmp.path().join("comply.toml"),
            format!("[rules.package-json-required-scripts]\nscripts = [{list}]\n"),
        )
        .expect("write cfg");
        let cfg = Config::load_from(tmp.path()).expect("load cfg");

        let path = Path::new("package.json");
        let ctx = CheckCtx {
            path,
            path_arc: Arc::from(path),
            source,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: crate::files::Language::Json,
        };
        Check.check(&ctx)
    }

    #[test]
    fn no_op_without_configured_list() {
        // Default config has an empty `scripts` list — the rule fires on nothing,
        // even on a package.json with no scripts at all.
        assert!(run_default(r#"{ "name": "pkg" }"#).is_empty());
        assert!(run_default(r#"{ "scripts": { "build": "tsc" } }"#).is_empty());
    }

    #[test]
    fn flags_single_missing_script() {
        let src = r#"{
  "name": "pkg",
  "scripts": {
    "build": "tsc"
  }
}"#;
        let diags = run_with_scripts(src, &["build", "test"]);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "The required script test is missing from package.json."
        );
    }

    #[test]
    fn flags_multiple_missing_scripts() {
        let src = r#"{
  "name": "pkg",
  "scripts": {
    "build": "tsc"
  }
}"#;
        let diags = run_with_scripts(src, &["build", "test", "lint"]);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "The required scripts test, lint are missing from package.json."
        );
    }

    #[test]
    fn flags_when_scripts_object_absent() {
        let src = r#"{ "name": "pkg" }"#;
        let diags = run_with_scripts(src, &["build"]);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].message,
            "The required script build is missing from package.json."
        );
    }

    #[test]
    fn clean_when_all_required_present() {
        let src = r#"{
  "name": "pkg",
  "scripts": {
    "build": "tsc",
    "test": "vitest",
    "lint": "eslint ."
  }
}"#;
        assert!(run_with_scripts(src, &["build", "test", "lint"]).is_empty());
    }

    #[test]
    fn ignores_non_package_json() {
        let tmp = TempDir::new().expect("tempdir");
        fs::write(
            tmp.path().join("comply.toml"),
            "[rules.package-json-required-scripts]\nscripts = [\"build\"]\n",
        )
        .expect("write cfg");
        let cfg = Config::load_from(tmp.path()).expect("load cfg");

        let path = Path::new("tsconfig.json");
        let ctx = CheckCtx {
            path,
            path_arc: Arc::from(path),
            source: r#"{ "name": "pkg" }"#,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: crate::files::Language::Json,
        };
        assert!(Check.check(&ctx).is_empty());
    }
}
