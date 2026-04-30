//! Shared verb detection used by both the Rust and TypeScript backends.
//!
//! Both languages walk string literals looking for `/api/createOrder`-style
//! URLs. The list of banned verbs and the matching algorithm are
//! identical — keeping them here means a verb added to the list lands in
//! both backends in one edit.

const BANNED_VERBS: &[&str] = &[
    "create", "get", "update", "delete", "remove", "list", "fetch", "find", "add", "set", "modify",
    "edit", "save", "load", "cancel", "refund", "submit", "approve", "reject", "archive",
];

const URL_NEEDLES: &[&str] = &["/api/", "/v1/", "/v2/"];

/// True if `text` looks like a REST URL containing one of the banned
/// verbs as a path segment prefix (followed by a CamelCase noun).
/// Returns the verb that matched.
#[must_use]
pub fn contains_verb_url(text: &str) -> Option<&'static str> {
    let inner = text.trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == 'r' || c == '#');
    if !URL_NEEDLES.iter().any(|n| inner.contains(n)) {
        return None;
    }
    for &verb in BANNED_VERBS {
        let vlen = verb.len();
        let mut start = 0;
        while let Some(idx) = inner[start..].find(verb) {
            let absolute = start + idx;
            let prev = absolute
                .checked_sub(1)
                .and_then(|i| inner.as_bytes().get(i));
            if prev != Some(&b'/') {
                start = absolute + vlen;
                continue;
            }
            let next = inner.as_bytes().get(absolute + vlen);
            if next.is_some_and(|b| b.is_ascii_uppercase()) {
                return Some(verb);
            }
            start = absolute + vlen;
        }
    }
    None
}
