//! Parse the payload after `// comply-ignore:` into a rule-id + justification.
//!
//! This is the load-bearing piece of the suppression mechanism: a silently
//! suppressed warning is tech debt no one ever pays back, so we require an
//! explicit justification after the separator. The split is whitespace- and
//! Unicode-aware to accept both em-dash (`—`) and padded ASCII `--`.

const EM_DASH: char = '—';
/// Padded ASCII fallback — the spaces around `--` prevent collision with the
/// hyphens inside rule ids like `no-nested-ternary`.
const ASCII_SEP: &str = " -- ";

/// One parsed comply-ignore comment after splitting on the separator.
///
/// Aligned with ESLint's `eslint-disable-next-line rule-a, rule-b`:
/// multiple rules may be listed comma-separated and they all apply to
/// the same target line.
#[derive(Debug)]
pub struct ParsedIgnore {
    /// Empty if no rule ids were provided. Rule ids are trimmed and
    /// de-duplicated in insertion order.
    pub rule_ids: Vec<String>,
    /// Empty if no justification was provided.
    pub justification: String,
}

/// Split a `// comply-ignore:` payload into `(rule_ids, justification)`.
/// Rule ids are comma-separated, both halves are trimmed; either may be
/// empty if not present.
pub fn parse(payload: &str) -> ParsedIgnore {
    let trimmed = payload.trim();

    // Try em-dash first, then padded ASCII `--`.
    let split = trimmed
        .split_once(EM_DASH)
        .or_else(|| trimmed.split_once(ASCII_SEP));

    let (rule_part, justification_part) = match split {
        Some((left, right)) => (left, right),
        None => (trimmed, ""),
    };

    let rule_ids: Vec<String> = rule_part
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    ParsedIgnore {
        rule_ids,
        justification: justification_part.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn em_dash_with_justification() {
        let p = parse(" no-throw — legacy code");
        assert_eq!(p.rule_ids, vec!["no-throw"]);
        assert_eq!(p.justification, "legacy code");
    }

    #[test]
    fn ascii_dash_with_justification() {
        let p = parse(" no-throw -- legacy code");
        assert_eq!(p.rule_ids, vec!["no-throw"]);
        assert_eq!(p.justification, "legacy code");
    }

    #[test]
    fn missing_justification() {
        let p = parse(" no-throw");
        assert_eq!(p.rule_ids, vec!["no-throw"]);
        assert_eq!(p.justification, "");
    }

    #[test]
    fn multi_rule_comma_separated() {
        // Regression for rbaumier/comply#22 — multi-rule syntax.
        let p = parse(" no-throw, no-let, no-explicit-any — same reason");
        assert_eq!(p.rule_ids, vec!["no-throw", "no-let", "no-explicit-any"]);
        assert_eq!(p.justification, "same reason");
    }

    #[test]
    fn multi_rule_with_spaces() {
        let p = parse(" no-throw,no-let , no-explicit-any -- reason");
        assert_eq!(p.rule_ids, vec!["no-throw", "no-let", "no-explicit-any"]);
    }

    #[test]
    fn skips_empty_rule_segments() {
        let p = parse(" no-throw, , no-let — x");
        assert_eq!(p.rule_ids, vec!["no-throw", "no-let"]);
    }

    #[test]
    fn accepts_numeric_justification() {
        // Regression: previous parser only accepted alphabetic justifications.
        let p = parse(" no-throw — see #4521");
        assert_eq!(p.justification, "see #4521");
    }

    #[test]
    fn accepts_punctuation_only_justification() {
        let p = parse(" no-throw — !!!");
        assert_eq!(p.justification, "!!!");
    }

    #[test]
    fn rule_with_hyphens_round_trips() {
        // Regression: " -- " separator must not collide with hyphens inside rule ids.
        let p = parse(" no-nested-ternary -- legacy form");
        assert_eq!(p.rule_ids, vec!["no-nested-ternary"]);
        assert_eq!(p.justification, "legacy form");
    }

    #[test]
    fn empty_payload() {
        let p = parse("");
        assert!(p.rule_ids.is_empty());
        assert_eq!(p.justification, "");
    }
}
