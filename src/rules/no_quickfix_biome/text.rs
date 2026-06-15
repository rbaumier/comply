//! no-quickfix-biome backend — flag the deprecated `quickfix.biome` code
//! action key inside editor settings files (VS Code / Zed).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::path::Path;

#[derive(Debug)]
pub struct Check;

/// Editor settings files this rule applies to. A file is in scope when its
/// path ends with one of these suffixes (matching Biome's `DEFAULT_PATHS`).
const SETTINGS_SUFFIXES: &[&str] = &[
    ".vscode/settings.json",
    "Code/User/settings.json",
    ".zed/settings.json",
    "zed/settings.json",
];

/// The deprecated key, written as it appears in source (a JSON object key).
const QUICKFIX_KEY: &str = "\"quickfix.biome\"";

fn is_settings_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    SETTINGS_SUFFIXES
        .iter()
        .any(|suffix| path_str.ends_with(suffix))
}

/// True when `"quickfix.biome"` at `idx` is used as an object key, i.e. the
/// next non-whitespace character after the closing quote is a `:`. This
/// excludes the string appearing as a value or inside an unrelated string.
fn is_object_key(line: &str, idx: usize) -> bool {
    let after = &line[idx + QUICKFIX_KEY.len()..];
    matches!(after.trim_start().as_bytes().first(), Some(b':'))
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["quickfix.biome"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_settings_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let mut search_from = 0;
            while let Some(rel) = line[search_from..].find(QUICKFIX_KEY) {
                let idx = search_from + rel;
                if is_object_key(line, idx) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: i + 1,
                        column: idx + 1,
                        rule_id: super::META.id.into(),
                        message:
                            "The use of `quickfix.biome` is deprecated; use `source.fixAll.biome` instead."
                                .to_string(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                search_from = idx + QUICKFIX_KEY.len();
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(path: &str, content: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new(path), content);
        Check.check(&ctx)
    }

    // --- INVALID (Biome fixtures: fires) ---

    #[test]
    fn flags_quickfix_biome_in_vscode_settings() {
        // Biome `simple/.vscode/settings.json`.
        let src = r#"{
  "editor.codeActionsOnSave": {
    "quickfix.biome": "explicit"
  }
}"#;
        let diags = check("/project/.vscode/settings.json", src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        assert!(diags[0].message.contains("quickfix.biome"));
    }

    #[test]
    fn flags_alongside_other_actions() {
        // Biome `more_items/.vscode/settings.json`.
        let src = r#"{
  "editor.codeActionsOnSave": {
    "source.organizeImports.biome": "explicit",
    "quickfix.biome": "explicit"
  }
}"#;
        let diags = check("/repo/.vscode/settings.json", src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 4);
    }

    #[test]
    fn flags_even_when_fix_all_present() {
        // Biome `has_fix_all/.vscode/settings.json` — still fires.
        let src = r#"{
  "editor.codeActionsOnSave": {
    "source.organizeImports.biome": "explicit",
    "source.fixAll.biome": "explicit",
    "quickfix.biome": "explicit"
  }
}"#;
        let diags = check("/repo/.vscode/settings.json", src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }

    #[test]
    fn flags_in_zed_settings() {
        // Biome `simple/.zed/settings.json` — Zed uses `true` as the value.
        let src = r#"{
  "editor.code_action_on_format": {
    "quickfix.biome": true
  }
}"#;
        let diags = check("/repo/.zed/settings.json", src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_in_code_user_settings() {
        let src = r#"{
  "editor.codeActionsOnSave": {
    "quickfix.biome": "explicit"
  }
}"#;
        let diags = check("/home/me/Code/User/settings.json", src);
        assert_eq!(diags.len(), 1);
    }

    // --- VALID (Biome fixtures: clean) ---

    #[test]
    fn allows_source_fix_all_biome() {
        let src = r#"{
  "editor.codeActionsOnSave": {
    "source.fixAll.biome": "explicit"
  }
}"#;
        assert!(check("/repo/.vscode/settings.json", src).is_empty());
    }

    #[test]
    fn allows_source_organize_imports_biome() {
        let src = r#"{
  "editor.codeActionsOnSave": {
    "source.organizeImports.biome": "explicit"
  }
}"#;
        assert!(check("/repo/.vscode/settings.json", src).is_empty());
    }

    #[test]
    fn ignores_non_settings_json() {
        // Same content, but not an editor settings file.
        let src = r#"{
  "editor.codeActionsOnSave": {
    "quickfix.biome": "explicit"
  }
}"#;
        assert!(check("/repo/package.json", src).is_empty());
        assert!(check("/repo/config/settings.json", src).is_empty());
    }

    #[test]
    fn ignores_settings_without_the_key() {
        let src = r#"{
  "editor.formatOnSave": true
}"#;
        assert!(check("/repo/.vscode/settings.json", src).is_empty());
    }

    // --- Over-firing guard ---

    #[test]
    fn ignores_quickfix_biome_as_a_value() {
        // The string appears as a value, not a key — must not fire.
        let src = r#"{
  "preferredAction": "quickfix.biome"
}"#;
        assert!(check("/repo/.vscode/settings.json", src).is_empty());
    }

    #[test]
    fn ignores_quickfix_biome_inside_unrelated_string() {
        let src = r#"{
  "note": "do not use quickfix.biome here"
}"#;
        assert!(check("/repo/.vscode/settings.json", src).is_empty());
    }
}
