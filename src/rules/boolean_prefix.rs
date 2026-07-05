//! Shared boolean-naming-convention predicate for JSX `&&`-guard rules.
//!
//! `jsx-ensure-booleans` and `react-no-and-conditional-jsx` both need to know
//! whether an operand is a boolean by naming convention: an identifier or a
//! call whose name carries a boolean prefix at a camelCase boundary
//! (`isSelected`, `hasFilters()`) evaluates to `boolean`, so `expr && <JSX/>`
//! cannot leak a literal `0`/`""`. Keeping the prefix list and the boundary
//! rule in one place keeps the two siblings in parity.

/// Prefixes that mark a value as boolean by naming convention.
const BOOLEAN_PREFIXES: &[&str] = &[
    "is", "has", "should", "can", "will", "did", "show", "hide", "with", "enable", "disable",
    "visible", "active", "open", "loading", "loaded", "allow", "need", "must",
];

/// True when `name` follows the boolean-naming convention: a boolean prefix at a
/// camelCase boundary (e.g. `isSelected`, `hasFilters`) or the bare prefix word
/// (`is`, `has`). Requiring an uppercase letter after the prefix avoids matching
/// words that merely begin with the letters (`island`, `cancel`, `history`).
#[must_use]
pub fn has_boolean_prefix(name: &str) -> bool {
    BOOLEAN_PREFIXES.iter().any(|p| {
        name.strip_prefix(p)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| c.is_uppercase()))
    })
}
