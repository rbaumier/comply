//! Parse ESLint inline config comments (`/* eslint <rule>: <severity> */`).
//!
//! ESLint lets a file turn a rule off in place with a block-comment config
//! directive: `/* eslint no-var: 0 */` or `/* eslint no-var: "off" */`. The
//! severity is `0`/`1`/`2` or `"off"`/`"warn"`/`"error"`, and a rule may carry
//! options as an array (`/* eslint no-magic-numbers: ["error", { ... }] */`)
//! whose first element is the severity. Codegen output (AWS SDK Smithy, etc.)
//! relies on this to silence rules it deliberately violates.
//!
//! comply honors only the **off** form (`0` / `"off"`), and only the block
//! comment shape — a rule set to a warning/error severity stays active, and the
//! directive comments `eslint-disable` / `eslint-enable` / `eslint-env` (a
//! hyphen, not whitespace, follows `eslint`) are not config comments and are
//! left to other layers. The directive applies to the whole file, matching the
//! requested behavior in rbaumier/comply#5510.

/// Collect every rule id set to the off severity (`0` / `"off"`) by an
/// `/* eslint ... */` config comment anywhere in `source`. A rule set to a
/// non-zero severity is not returned, so an active rule is never suppressed.
///
/// The scan is purely lexical and not string-literal aware: a verbatim
/// `/* eslint <rule>: 0 */` byte sequence sitting inside a string literal would
/// be read as a config comment. The only consequence is under-reporting that
/// rule in that file (never a false positive), and an off-severity config
/// comment embedded in a string is not a shape real code produces.
pub fn off_rules(source: &str) -> Vec<String> {
    let mut off = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("/*") {
        let after_open = &rest[start + 2..];
        let Some(close) = after_open.find("*/") else {
            break;
        };
        let body = &after_open[..close];
        collect_off_rules_from_comment(body, &mut off);
        rest = &after_open[close + 2..];
    }
    off
}

/// Append the off-severity rule ids declared inside one block comment's body
/// (the text between `/*` and `*/`). A no-op unless the body is an `eslint`
/// config comment.
fn collect_off_rules_from_comment(body: &str, off: &mut Vec<String>) {
    let trimmed = body.trim();
    // The config form is `eslint` followed by whitespace then the rule list.
    // `eslint-disable` / `eslint-enable` / `eslint-env` have a hyphen after
    // `eslint`, so requiring whitespace excludes those directive comments.
    let Some(after_eslint) = trimmed.strip_prefix("eslint") else {
        return;
    };
    if !after_eslint.starts_with(|c: char| c.is_whitespace()) {
        return;
    }
    let config = after_eslint.trim_start();
    for entry in split_top_level_commas(config) {
        if let Some((rule, value)) = entry.split_once(':')
            && is_off_severity(value.trim())
        {
            let rule = rule.trim();
            if !rule.is_empty() {
                off.push(rule.to_string());
            }
        }
    }
}

/// True when an ESLint severity value means "off": the scalar `0` / `"off"` /
/// `'off'`, or an options array whose first element is one of those.
fn is_off_severity(value: &str) -> bool {
    let value = value.strip_prefix('[').map(str::trim_start).unwrap_or(value);
    // The severity is the first array element or the whole scalar value; stop at
    // the comma that separates it from the options object.
    let token = value.split(',').next().unwrap_or(value).trim();
    let unquoted = token
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .or_else(|| token.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')))
        .unwrap_or(token);
    unquoted == "0" || unquoted.eq_ignore_ascii_case("off")
}

/// Split an eslint config body on commas that separate `rule: severity`
/// entries, ignoring commas nested inside an options array (`[...]`) or object
/// (`{...}`) so `no-magic-numbers: ["error", { ... }]` stays one entry.
fn split_top_level_commas(config: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in config.char_indices() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth <= 0 => {
                entries.push(&config[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    entries.push(&config[start..]);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_zero_severity_disables_rule() {
        assert_eq!(off_rules("/* eslint no-var: 0 */"), vec!["no-var"]);
    }

    #[test]
    fn off_string_severity_disables_rule() {
        assert_eq!(off_rules("/* eslint no-var: \"off\" */"), vec!["no-var"]);
        assert_eq!(off_rules("/* eslint no-var: 'off' */"), vec!["no-var"]);
    }

    #[test]
    fn non_zero_severity_does_not_disable() {
        assert!(off_rules("/* eslint no-var: 2 */").is_empty());
        assert!(off_rules("/* eslint no-var: 1 */").is_empty());
        assert!(off_rules("/* eslint no-var: \"error\" */").is_empty());
        assert!(off_rules("/* eslint no-var: \"warn\" */").is_empty());
    }

    #[test]
    fn multiple_rules_in_one_comment() {
        let off = off_rules("/* eslint no-var: 0, no-magic-numbers: 0 */");
        assert_eq!(off, vec!["no-var", "no-magic-numbers"]);
    }

    #[test]
    fn mixed_severities_only_returns_off_rules() {
        let off = off_rules("/* eslint no-var: 0, no-console: 2 */");
        assert_eq!(off, vec!["no-var"]);
    }

    #[test]
    fn options_array_with_error_severity_does_not_disable() {
        // The nested object's comma must not split the entry, and `"error"`
        // first element keeps the rule active.
        assert!(
            off_rules("/* eslint no-magic-numbers: [\"error\", { \"ignore\": [0] }] */").is_empty()
        );
    }

    #[test]
    fn options_array_with_off_severity_disables() {
        let off = off_rules("/* eslint no-magic-numbers: [\"off\", { \"ignore\": [0] }] */");
        assert_eq!(off, vec!["no-magic-numbers"]);
    }

    #[test]
    fn eslint_disable_directive_is_not_a_config_comment() {
        // `eslint-disable` has a hyphen after `eslint`, not whitespace.
        assert!(off_rules("/* eslint-disable no-var */").is_empty());
        assert!(off_rules("/* eslint-enable */").is_empty());
        assert!(off_rules("/* eslint-env browser */").is_empty());
    }

    #[test]
    fn unrelated_block_comment_is_ignored() {
        assert!(off_rules("/* this file uses var on purpose */").is_empty());
    }

    #[test]
    fn line_comment_form_is_not_honored() {
        // ESLint config comments are block comments only.
        assert!(off_rules("// eslint no-var: 0").is_empty());
    }
}
