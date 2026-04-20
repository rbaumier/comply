//! Default configuration — loaded from the embedded `defaults.toml`.
//!
//! The file is the single source of truth for every built-in threshold
//! (max lines, max depth, id-length min, etc.). Keeping it as TOML
//! means:
//!
//! 1. Adding a new knob = one TOML line, zero Rust changes.
//! 2. `comply config print` writes the file back verbatim, so the
//!    documentation-as-code stays in sync.
//! 3. The parser path for defaults is exactly the parser path for
//!    user config — if `defaults.toml` is invalid, the build test
//!    catches it rather than an end-user install.

use super::schema::ComplyToml;

const DEFAULTS_TOML: &str = include_str!("defaults.toml");

/// Parse the embedded `defaults.toml`. A panic here means the TOML
/// shipped with the binary is malformed, which the `parses_cleanly`
/// test catches at build time.
#[must_use]
pub fn build_default_config() -> ComplyToml {
    toml::from_str(DEFAULTS_TOML)
        .expect("embedded defaults.toml must parse — see defaults.rs::parses_cleanly") // comply-ignore: rust-no-unwrap — compile-time-checked constant.
}

/// Raw text of the embedded defaults — used by `comply config print`
/// to dump the canonical `comply.toml` template complete with
/// comments.
#[must_use]
pub fn default_toml_text() -> &'static str {
    DEFAULTS_TOML
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cleanly() {
        // If this fails, defaults.toml has a syntax error. Fix the
        // TOML rather than the test.
        let _ = build_default_config();
    }

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
        let rule = cfg.rules.get("no-multi-op-oneliner").expect("rule present");
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

    #[test]
    fn defaults_include_id_length_min() {
        let cfg = build_default_config();
        let min = cfg
            .rules
            .get("id-length")
            .and_then(|r| r.extra.get("min"))
            .and_then(toml::Value::as_integer);
        assert_eq!(min, Some(2));
    }
}
