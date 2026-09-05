//! Shared boolean-naming-convention predicate for JSX `&&`-guard rules.
//!
//! `jsx-ensure-booleans` and `react-no-and-conditional-jsx` both need to know
//! whether an operand is a boolean by naming convention: an identifier, member
//! or call whose name opens on a boolean prefix word (`isSelected`,
//! `props.showText`, `hasFilters()`) evaluates to `boolean`, so `expr && <JSX/>`
//! cannot leak a literal `0`/`""`. Keeping the prefix list and the boundary
//! rule in one place keeps the two siblings in parity.

/// Prefixes that mark a value as boolean by naming convention.
const BOOLEAN_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "show", "hide", "with", "enable", "disable",
    "visible", "active", "open", "loading", "loaded", "allow", "need", "must",
];

/// True when `name` follows the boolean-naming convention: its first word is a
/// boolean prefix (`isSelected`, `has_filters`, `SHOULD_RETRY`, bare `is`).
/// Keying on the whole first word rather than a leading substring avoids
/// matching words that merely begin with the letters (`island`, `cancel`,
/// `hasty`).
#[must_use]
pub fn has_boolean_prefix(name: &str) -> bool {
    let first = first_word(name);
    BOOLEAN_PREFIXES.iter().any(|p| first.eq_ignore_ascii_case(p))
}

/// The leading word of an identifier, i.e. the prefix ending at the first word
/// boundary: a separator (`_`, `-`, `$`, digit) or a lowercase-to-uppercase
/// transition (`isReady` → `is`, `IS_READY` → `IS`, `island` → `island`).
fn first_word(name: &str) -> &str {
    let bytes = name.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        let is_separator = matches!(byte, b'_' | b'-' | b'$') || byte.is_ascii_digit();
        let is_camel_hump =
            index > 0 && byte.is_ascii_uppercase() && bytes[index - 1].is_ascii_lowercase();
        if is_separator || is_camel_hump {
            return &name[..index];
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::has_boolean_prefix;

    #[test]
    fn accepts_prefix_at_a_word_boundary() {
        assert!(has_boolean_prefix("isSelected"));
        assert!(has_boolean_prefix("hasFilters"));
        assert!(has_boolean_prefix("withTimeBubble"));
        assert!(has_boolean_prefix("has2Items"));
        assert!(has_boolean_prefix("is"));
        assert!(has_boolean_prefix("is_ready"));
        assert!(has_boolean_prefix("SHOULD_RETRY"));
    }

    #[test]
    fn rejects_a_longer_word_starting_with_the_prefix_letters() {
        assert!(!has_boolean_prefix("island"));
        assert!(!has_boolean_prefix("cancel"));
        assert!(!has_boolean_prefix("hasty"));
        assert!(!has_boolean_prefix("canvas"));
        assert!(!has_boolean_prefix("ISLAND"));
        assert!(!has_boolean_prefix("islandTile"));
    }

    #[test]
    fn rejects_a_name_without_a_boolean_prefix() {
        assert!(!has_boolean_prefix("items"));
        assert!(!has_boolean_prefix("count"));
        assert!(!has_boolean_prefix(""));
    }
}
