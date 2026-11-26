//! Remap table: oxlint's reported diagnostic `code` → comply's RuleMeta.
//!
//! oxlint reports a rule as `plugin(rule)`, like `typescript(foo)`.
//! comply's config names it `typescript/foo`.
//! This module translates between the two.
//!
//! Without the remap, user-facing diagnostics would show oxlint's verbose
//! code and oxlint's generic message instead of comply's stable rule id
//! and our remediation wording.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;

/// Build a lookup table from oxlint's reported `code` string to the comply
/// RuleMeta that owns it.
pub fn build_table(
    bindings: &[(&'static str, &'static RuleMeta, Severity)],
) -> FxHashMap<String, &'static RuleMeta> {
    let mut table = FxHashMap::with_capacity_and_hasher(bindings.len(), Default::default());
    for (key, meta, _) in bindings {
        table.insert(config_key_to_oxlint_code(key), *meta);
    }
    table
}

/// The code oxlint 1.90 reports for a comply config key.
/// `nextjs/` reports as `next(…)`, `react/exhaustive-deps` as `react-hooks(…)`.
/// A key without a plugin is an eslint core rule.
pub fn config_key_to_oxlint_code(config_key: &str) -> String {
    match config_key.split_once('/') {
        Some(("nextjs", rule)) => format!("next({rule})"),
        Some(("react", "exhaustive-deps")) => "react-hooks(exhaustive-deps)".to_string(),
        Some((plugin, rule)) => format!("{plugin}({rule})"),
        None => format!("eslint({config_key})"),
    }
}

/// `code` as oxlint 1.90 writes it.
/// oxlint 1.59 wrote `typescript-eslint(…)` and `eslint-plugin-<plugin>(…)`.
pub fn current_code(code: &str) -> Cow<'_, str> {
    match code.strip_prefix("typescript-eslint(") {
        Some(rule) => Cow::Owned(format!("typescript({rule}")),
        None => Cow::Borrowed(code.strip_prefix("eslint-plugin-").unwrap_or(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_key_transforms_match_oxlint_codes() {
        assert_eq!(
            config_key_to_oxlint_code("typescript/no-explicit-any"),
            "typescript(no-explicit-any)"
        );
        assert_eq!(
            config_key_to_oxlint_code("import/no-default-export"),
            "import(no-default-export)"
        );
        assert_eq!(
            config_key_to_oxlint_code("unicorn/no-array-for-each"),
            "unicorn(no-array-for-each)"
        );
        assert_eq!(
            config_key_to_oxlint_code("promise/catch-or-return"),
            "promise(catch-or-return)"
        );
        assert_eq!(
            config_key_to_oxlint_code("vue/no-import-compiler-macros"),
            "vue(no-import-compiler-macros)"
        );
        assert_eq!(
            config_key_to_oxlint_code("nextjs/no-before-interactive-script-outside-document"),
            "next(no-before-interactive-script-outside-document)"
        );
        assert_eq!(
            config_key_to_oxlint_code("react/jsx-curly-brace-presence"),
            "react(jsx-curly-brace-presence)"
        );
        assert_eq!(
            config_key_to_oxlint_code("react/exhaustive-deps"),
            "react-hooks(exhaustive-deps)"
        );
        assert_eq!(
            config_key_to_oxlint_code("jest/no-export"),
            "jest(no-export)"
        );
        assert_eq!(
            config_key_to_oxlint_code("oxc/no-barrel-file"),
            "oxc(no-barrel-file)"
        );
        assert_eq!(config_key_to_oxlint_code("eqeqeq"), "eslint(eqeqeq)");
    }
}
