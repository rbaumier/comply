//! Banned-abbreviation dictionary and identifier segmentation, shared by the
//! Rust and TypeScript backends.
//!
//! Both backends answer the same question on the same word boundaries, so the
//! dictionary and the splitter live here and a new entry lands in both in one
//! edit.
//!
//! ## Entry selection criteria
//!
//! An entry is included only when it has a single canonical expansion a reader
//! cannot guess wrong. An abbreviation that is an ecosystem idiom, or that
//! expands differently per domain, produces more noise than signal.
//!
//! ### Rejected entries and why
//!
//! - `cfg`, `ctx`, `idx`, `err`, `fmt`, `ret`, `val`, `num`, `str`, `obj`,
//!   `arr`, `req`, `res`, `msg`, `auth`, `db`, `dict` — part of the Rust and
//!   TypeScript vocabulary (`#[cfg]` attributes, `std::fmt`, io context,
//!   iteration index).
//! - `addr` — standard Rust API: `std::net::SocketAddr`, `peer_addr()`,
//!   `local_addr()`, `bind_addr`.
//! - `org` — the canonical domain term of the GitHub API (`GET /orgs/{org}`,
//!   `org_member`) and of multi-tenant SaaS schemas (`org_id`).
//! - `desc` — a descriptor in virtualization and device-driver protocols
//!   (VirtIO, USB, PCIe) and the SQL `ORDER BY … DESC` keyword, so
//!   'description' is frequently the wrong expansion.
//! - `pwd` — the Unix `pwd(1)` command and `$PWD` variable in shell and
//!   filesystem code, a password in URL and auth code, and `struct passwd` in
//!   POSIX bindings.
//!
//! A project that wants any of these banned adds it to `banned` in its own
//! `comply.toml`.

const DEFAULT_BANNED: &[(&str, &str)] = &[
    ("acct", "account"),
    ("usr", "user"),
    ("btn", "button"),
    ("cnt", "count"),
];

/// Merge the project's `banned` entries into the default dictionary. Each extra
/// entry reads `abbreviation:full word`; a malformed or already-known entry is
/// dropped.
#[must_use]
pub fn build_banned_list(extra: &[String]) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = DEFAULT_BANNED
        .iter()
        .map(|(abbreviation, full)| ((*abbreviation).to_owned(), (*full).to_owned()))
        .collect();
    for entry in extra {
        let Some((abbreviation, full)) = entry.split_once(':') else {
            continue;
        };
        let abbreviation = abbreviation.trim().to_lowercase();
        let full = full.trim().to_owned();
        if !list.iter().any(|(known, _)| *known == abbreviation) {
            list.push((abbreviation, full));
        }
    }
    list
}

/// Return the banned entry matched by a whole segment of `name`, so `usr_id`
/// matches `usr` while `accountant` matches nothing.
#[must_use]
pub fn matches_banned(name: &str, banned: &[(String, String)]) -> Option<(String, String)> {
    for segment in split_words(name) {
        let lowered = segment.to_ascii_lowercase();
        if let Some(pair) = banned.iter().find(|(abbreviation, _)| lowered == *abbreviation) {
            return Some(pair.clone());
        }
    }
    None
}

/// Split an identifier into its segments on both an underscore and a
/// lowercase-to-uppercase boundary (`UsrProfile` → `["Usr", "Profile"]`,
/// `MANAGE_ORG` → `["MANAGE", "ORG"]`).
///
/// An uppercase run stays one segment, so an acronym such as `HTTPServer`
/// yields `["HTTPServer"]` rather than a spurious `HTTP` boundary.
fn split_words(name: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let bytes = name.as_bytes();
    let mut start = 0;
    for i in 1..bytes.len() {
        let previous_is_lower = bytes[i - 1].is_ascii_lowercase();
        let current_is_upper = bytes[i].is_ascii_uppercase();
        let current_is_underscore = bytes[i] == b'_';
        if (previous_is_lower && current_is_upper) || current_is_underscore {
            words.push(&name[start..i]);
            start = if current_is_underscore { i + 1 } else { i };
        }
    }
    if start < bytes.len() {
        words.push(&name[start..]);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(abbreviation, full)| ((*abbreviation).to_owned(), (*full).to_owned()))
            .collect()
    }

    #[test]
    fn splits_on_underscore_and_camel_boundaries() {
        assert_eq!(split_words("usr_id"), vec!["usr", "id"]);
        assert_eq!(split_words("UsrProfile"), vec!["Usr", "Profile"]);
        assert_eq!(split_words("MANAGE_ORG"), vec!["MANAGE", "ORG"]);
        assert_eq!(split_words("organ"), vec!["organ"]);
    }

    #[test]
    fn keeps_an_uppercase_run_as_one_segment() {
        assert_eq!(split_words("HTTPServer"), vec!["HTTPServer"]);
    }

    #[test]
    fn matches_a_pascal_case_segment() {
        let list = banned(&[("msg", "message")]);
        assert_eq!(
            matches_banned("MsgKind", &list),
            Some(("msg".to_owned(), "message".to_owned()))
        );
    }

    #[test]
    fn ignores_a_segment_that_merely_contains_the_abbreviation() {
        let list = banned(&[("org", "organization"), ("or", "or")]);
        assert_eq!(matches_banned("organ", &list), None);
        assert_eq!(matches_banned("story", &list), None);
    }

    #[test]
    fn merges_project_entries_without_duplicating_defaults() {
        let list = build_banned_list(&["msg:message".to_owned(), "usr:person".to_owned()]);
        assert!(list.contains(&("msg".to_owned(), "message".to_owned())));
        assert_eq!(list.iter().filter(|(abbreviation, _)| abbreviation == "usr").count(), 1);
    }

    #[test]
    fn drops_a_project_entry_without_a_separator() {
        let list = build_banned_list(&["mgr".to_owned()]);
        assert!(!list.iter().any(|(abbreviation, _)| abbreviation == "mgr"));
    }
}
