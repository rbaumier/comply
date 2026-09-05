//! Word-token splitting for identifiers.
//!
//! A word must match as a token, not a substring.
//! `recsv` is no CSV name; `Reprint` is no request.

/// Splits an identifier into word tokens.
/// Boundaries are camelCase and non-alphanumeric separators.
/// `buildCsvRow` → `build`, `Csv`, `Row`.
/// An uppercase run stays whole: `RequestDTO` → `Request`, `DTO`.
pub fn split_identifier_words(name: &str) -> impl Iterator<Item = &str> {
    let bytes = name.as_bytes();
    let mut start = 0;
    let mut boundaries = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        let is_sep = !b.is_ascii_alphanumeric();
        let is_camel_boundary =
            i > 0 && b.is_ascii_uppercase() && bytes[i - 1].is_ascii_lowercase();
        if is_sep {
            if start < i {
                boundaries.push((start, i));
            }
            start = i + 1;
        } else if is_camel_boundary {
            boundaries.push((start, i));
            start = i;
        }
    }
    if start < bytes.len() {
        boundaries.push((start, bytes.len()));
    }
    boundaries.into_iter().map(move |(s, e)| &name[s..e])
}

#[cfg(test)]
mod tests {
    use super::split_identifier_words;

    fn words(name: &str) -> Vec<&str> {
        split_identifier_words(name).collect()
    }

    #[test]
    fn splits_camel_case() {
        assert_eq!(words("buildCsvRow"), ["build", "Csv", "Row"]);
    }

    #[test]
    fn splits_on_separators() {
        assert_eq!(words("tgt_lang-token.id"), ["tgt", "lang", "token", "id"]);
    }

    #[test]
    fn keeps_uppercase_runs_whole() {
        assert_eq!(words("RequestDTO"), ["Request", "DTO"]);
    }

    #[test]
    fn yields_nothing_for_an_empty_name() {
        assert!(words("").is_empty());
    }
}
