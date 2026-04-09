//! Remap table: oxlint's reported diagnostic `code` → comply's RuleMeta.
//!
//! Oxlint emits rule ids in the format `plugin-name(rule-name)` — the
//! plugin name depends on the plugin: `typescript-eslint(foo)` for ts rules,
//! `eslint-plugin-import(foo)` for import rules, etc. comply's config uses
//! the shorter plugin prefix `typescript/foo`. This module translates
//! between the two.
//!
//! Without the remap, user-facing diagnostics would show oxlint's verbose
//! code and oxlint's generic message instead of comply's stable rule id
//! and our remediation wording.

use std::collections::HashMap;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;

/// Build a lookup table from oxlint's reported `code` string to the comply
/// RuleMeta that owns it.
pub fn build_table(
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
) -> HashMap<String, &'static RuleMeta> {
    let mut table = HashMap::with_capacity(bindings.len());
    for (key, meta, _) in bindings {
        table.insert(config_key_to_oxlint_code(key), *meta);
    }
    table
}

/// Translate a comply oxlint config key into oxlint's reported diagnostic code.
/// Plugin prefix translations verified empirically against oxlint 1.59:
/// - `typescript/*` → `typescript-eslint(*)`
/// - `import/*` → `eslint-plugin-import(*)`
/// - `unicorn/*` → `eslint-plugin-unicorn(*)`
/// - `promise/*` → `eslint-plugin-promise(*)`
/// - `oxc/*` → `oxc(*)`
/// - bare eslint core → `eslint(*)`
pub fn config_key_to_oxlint_code(config_key: &str) -> String {
    if let Some(rest) = config_key.strip_prefix("typescript/") {
        return format!("typescript-eslint({rest})");
    }
    if let Some(rest) = config_key.strip_prefix("import/") {
        return format!("eslint-plugin-import({rest})");
    }
    if let Some(rest) = config_key.strip_prefix("unicorn/") {
        return format!("eslint-plugin-unicorn({rest})");
    }
    if let Some(rest) = config_key.strip_prefix("promise/") {
        return format!("eslint-plugin-promise({rest})");
    }
    if let Some(rest) = config_key.strip_prefix("oxc/") {
        return format!("oxc({rest})");
    }
    format!("eslint({config_key})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_key_transforms_match_oxlint_codes() {
        assert_eq!(
            config_key_to_oxlint_code("typescript/no-explicit-any"),
            "typescript-eslint(no-explicit-any)"
        );
        assert_eq!(
            config_key_to_oxlint_code("import/no-default-export"),
            "eslint-plugin-import(no-default-export)"
        );
        assert_eq!(
            config_key_to_oxlint_code("unicorn/no-array-for-each"),
            "eslint-plugin-unicorn(no-array-for-each)"
        );
        assert_eq!(
            config_key_to_oxlint_code("promise/catch-or-return"),
            "eslint-plugin-promise(catch-or-return)"
        );
        assert_eq!(
            config_key_to_oxlint_code("oxc/no-barrel-file"),
            "oxc(no-barrel-file)"
        );
        assert_eq!(config_key_to_oxlint_code("eqeqeq"), "eslint(eqeqeq)");
    }
}
