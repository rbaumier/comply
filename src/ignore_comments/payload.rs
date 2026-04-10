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
#[derive(Debug)]
pub struct ParsedIgnore {
    pub rule_id: String,
    /// Empty if no justification was provided.
    pub justification: String,
}

/// Split a `// comply-ignore:` payload into `(rule_id, justification)`.
/// Both are trimmed; either may be empty if not present.
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

    ParsedIgnore {
        rule_id: rule_part.trim().to_string(),
        justification: justification_part.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn em_dash_with_justification() {
        let p = parse(" no-throw — legacy code");
        assert_eq!(p.rule_id, "no-throw");
        assert_eq!(p.justification, "legacy code");
    }

    #[test]
    fn ascii_dash_with_justification() {
        let p = parse(" no-throw -- legacy code");
        assert_eq!(p.rule_id, "no-throw");
        assert_eq!(p.justification, "legacy code");
    }

    #[test]
    fn missing_justification() {
        let p = parse(" no-throw");
        assert_eq!(p.rule_id, "no-throw");
        assert_eq!(p.justification, "");
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
        assert_eq!(p.rule_id, "no-nested-ternary");
        assert_eq!(p.justification, "legacy form");
    }

    #[test]
    fn empty_payload() {
        let p = parse("");
        assert_eq!(p.rule_id, "");
        assert_eq!(p.justification, "");
    }
}
