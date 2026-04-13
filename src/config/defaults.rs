//! Hardcoded default thresholds for every rule that exposes one.
//!
//! These are the values used when neither `comply.toml` nor a CLI flag
//! overrides the rule. Keeping them in one file makes it trivial to:
//!   1. generate the default `comply.toml` via `comply config init`
//!   2. unit-test that rules and config stay in sync
//!   3. document every knob in one place

use std::collections::HashMap;

use super::schema::{ComplyToml, RuleConfig};

/// Build the default config — every rule with a threshold gets its
/// canonical value here. Adding a new threshold-rule means appending
/// one entry to this function and reading it via `Config::threshold`
/// from inside the rule's check.
pub fn build_default_config() -> ComplyToml {
    let mut rules: HashMap<String, RuleConfig> = HashMap::new();

    insert_threshold(&mut rules, "max-file-lines", "max", 200);
    insert_threshold(&mut rules, "max-function-lines", "max", 30);
    insert_threshold(&mut rules, "no-multi-op-oneliner", "min_ops", 6);
    insert_threshold(&mut rules, "no-multi-op-oneliner", "min_line_length", 80);
    insert_threshold(&mut rules, "prefer-switch-over-chained-if", "min_arms", 4);
    insert_threshold(&mut rules, "rust-no-large-tuple-return", "max_elements", 3);

    // Cross-language thresholds that propagate into the oxlint/clippy
    // command line — kept here so the same authoritative number flows
    // into both the generated oxlintrc and the `-W` flag list.
    insert_threshold(&mut rules, "max-params", "max", 3);
    insert_threshold(&mut rules, "max-depth", "max", 3);
    insert_threshold(&mut rules, "id-length", "min", 2);

    ComplyToml {
        rules,
        overrides: HashMap::new(),
    }
}

/// Insert a `<key> = <value>` knob into the rule's `extra` map. Creates
/// the rule's entry if it doesn't exist yet, so multiple thresholds
/// for the same rule (e.g. `min_ops` + `min_line_length`) compose.
fn insert_threshold(rules: &mut HashMap<String, RuleConfig>, rule_id: &str, key: &str, value: i64) {
    let entry = rules.entry(rule_id.to_string()).or_default();
    entry
        .extra
        .insert(key.to_string(), toml::Value::Integer(value));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_include_max_function_lines() {
        let cfg = build_default_config();
        let max = cfg
            .rules
            .get("max-function-lines")
            .and_then(|r| r.extra.get("max"))
            .and_then(toml::Value::as_integer);
        assert_eq!(max, Some(30));
    }

    #[test]
    fn defaults_include_no_multi_op_oneliner_two_thresholds() {
        let cfg = build_default_config();
        let rule = cfg.rules.get("no-multi-op-oneliner").unwrap();
        assert_eq!(
            rule.extra.get("min_ops").and_then(toml::Value::as_integer),
            Some(6)
        );
        assert_eq!(
            rule.extra
                .get("min_line_length")
                .and_then(toml::Value::as_integer),
            Some(80)
        );
    }
}
