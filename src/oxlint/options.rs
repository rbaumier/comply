//! Per-rule option derivation for the generated oxlintrc.
//!
//! Infrastructure for translating `[rules.<id>]` TOML sections into
//! ESLint-style `[severity, options]` tuples. Currently no rules use
//! this path — `id-length` had been the only user and was rewritten
//! as a native check in `src/rules/id_length/` so the diagnostic
//! message could name the offending identifier. The module stays
//! wired up so adding a new option-bearing oxlint rule is a one-line
//! match arm rather than re-plumbing the whole pipeline.

use serde_json::Value;

use crate::config::Config;

/// Return the options object to pass to oxlint for `rule_id`, or
/// `None` if this rule is severity-only.
#[must_use]
pub fn for_rule(_rule_id: &str, _config: &Config) -> Option<Value> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_static_config;

    #[test]
    fn no_rules_are_option_bearing_yet() {
        assert!(for_rule("id-length", default_static_config()).is_none());
        assert!(for_rule("no-await-in-loop", default_static_config()).is_none());
    }
}
