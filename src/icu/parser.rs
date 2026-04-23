//! ICU Message Format parser.
//!
//! Supports:
//! - Simple arguments: `{name}`
//! - Plural: `{count, plural, =0 {none} one {one} other {many}}`
//! - Select: `{gender, select, male {He} female {She} other {They}}`
//! - SelectOrdinal: `{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}`
//! - Nested messages within plural/select branches

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcuError {
    pub kind: IcuErrorKind,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcuErrorKind {
    UnclosedBrace,
    UnmatchedCloseBrace,
    EmptyArgument,
    InvalidPluralKeyword(String),
    MissingOtherCategory,
    InvalidSelectSyntax,
    ExpectedComma,
    ExpectedBraceOrKeyword,
}

impl fmt::Display for IcuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            IcuErrorKind::UnclosedBrace => write!(f, "unclosed brace at position {}", self.position),
            IcuErrorKind::UnmatchedCloseBrace => write!(f, "unmatched closing brace at position {}", self.position),
            IcuErrorKind::EmptyArgument => write!(f, "empty argument at position {}", self.position),
            IcuErrorKind::InvalidPluralKeyword(kw) => write!(f, "invalid plural keyword `{kw}` at position {}", self.position),
            IcuErrorKind::MissingOtherCategory => write!(f, "`other` category is required at position {}", self.position),
            IcuErrorKind::InvalidSelectSyntax => write!(f, "invalid select syntax at position {}", self.position),
            IcuErrorKind::ExpectedComma => write!(f, "expected comma at position {}", self.position),
            IcuErrorKind::ExpectedBraceOrKeyword => write!(f, "expected brace or keyword at position {}", self.position),
        }
    }
}

const PLURAL_KEYWORDS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

pub fn parse(input: &str) -> Result<(), IcuError> {
    let bytes = input.as_bytes();
    let mut i = 0;
    parse_message(bytes, &mut i, 0)?;
    Ok(())
}

fn parse_message(bytes: &[u8], i: &mut usize, depth: usize) -> Result<(), IcuError> {
    while *i < bytes.len() {
        match bytes[*i] {
            b'{' => {
                let start = *i;
                *i += 1;
                parse_argument(bytes, i, start)?;
            }
            b'}' => {
                if depth == 0 {
                    return Err(IcuError {
                        kind: IcuErrorKind::UnmatchedCloseBrace,
                        position: *i,
                    });
                }
                return Ok(());
            }
            b'\'' => {
                // Escaped sequence - skip quoted content
                *i += 1;
                if *i < bytes.len() && bytes[*i] == b'\'' {
                    // Double quote = literal quote
                    *i += 1;
                } else {
                    // Skip until closing quote
                    while *i < bytes.len() && bytes[*i] != b'\'' {
                        *i += 1;
                    }
                    if *i < bytes.len() {
                        *i += 1;
                    }
                }
            }
            _ => *i += 1,
        }
    }
    Ok(())
}

fn parse_argument(bytes: &[u8], i: &mut usize, start: usize) -> Result<(), IcuError> {
    skip_whitespace(bytes, i);

    if *i >= bytes.len() {
        return Err(IcuError {
            kind: IcuErrorKind::UnclosedBrace,
            position: start,
        });
    }

    // Check for empty argument
    if bytes[*i] == b'}' {
        return Err(IcuError {
            kind: IcuErrorKind::EmptyArgument,
            position: start,
        });
    }

    // Read argument name
    let name_start = *i;
    while *i < bytes.len() && is_arg_name_char(bytes[*i]) {
        *i += 1;
    }

    if *i == name_start {
        return Err(IcuError {
            kind: IcuErrorKind::EmptyArgument,
            position: start,
        });
    }

    skip_whitespace(bytes, i);

    if *i >= bytes.len() {
        return Err(IcuError {
            kind: IcuErrorKind::UnclosedBrace,
            position: start,
        });
    }

    // Simple argument: {name}
    if bytes[*i] == b'}' {
        *i += 1;
        return Ok(());
    }

    // Complex argument: {name, type, ...}
    if bytes[*i] != b',' {
        return Err(IcuError {
            kind: IcuErrorKind::ExpectedComma,
            position: *i,
        });
    }
    *i += 1;
    skip_whitespace(bytes, i);

    // Read type (plural, select, selectordinal, number, date, time)
    let type_start = *i;
    while *i < bytes.len() && is_arg_name_char(bytes[*i]) {
        *i += 1;
    }
    let arg_type = std::str::from_utf8(&bytes[type_start..*i]).unwrap_or("");

    skip_whitespace(bytes, i);

    match arg_type {
        "plural" | "selectordinal" => parse_plural(bytes, i, start),
        "select" => parse_select(bytes, i, start),
        "number" | "date" | "time" | "spellout" | "ordinal" | "duration" => {
            // These may have optional style after comma, or just close
            parse_simple_format(bytes, i, start)
        }
        _ => {
            // Unknown type - just find matching close brace
            parse_simple_format(bytes, i, start)
        }
    }
}

fn parse_simple_format(bytes: &[u8], i: &mut usize, start: usize) -> Result<(), IcuError> {
    // Skip optional style and find closing brace
    let mut depth = 1;
    while *i < bytes.len() && depth > 0 {
        match bytes[*i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\'' => {
                *i += 1;
                // Skip quoted content
                while *i < bytes.len() && bytes[*i] != b'\'' {
                    *i += 1;
                }
            }
            _ => {}
        }
        *i += 1;
    }
    if depth > 0 {
        return Err(IcuError {
            kind: IcuErrorKind::UnclosedBrace,
            position: start,
        });
    }
    Ok(())
}

fn parse_plural(bytes: &[u8], i: &mut usize, start: usize) -> Result<(), IcuError> {
    if *i >= bytes.len() || bytes[*i] != b',' {
        return Err(IcuError {
            kind: IcuErrorKind::ExpectedComma,
            position: *i,
        });
    }
    *i += 1;
    skip_whitespace(bytes, i);

    let mut has_other = false;

    // Parse plural cases: =N {msg} or keyword {msg}
    loop {
        skip_whitespace(bytes, i);

        if *i >= bytes.len() {
            return Err(IcuError {
                kind: IcuErrorKind::UnclosedBrace,
                position: start,
            });
        }

        if bytes[*i] == b'}' {
            *i += 1;
            break;
        }

        // Read keyword (=0, =1, zero, one, two, few, many, other)
        let kw_start = *i;
        if bytes[*i] == b'=' {
            *i += 1;
            // Exact match: =N
            while *i < bytes.len() && bytes[*i].is_ascii_digit() {
                *i += 1;
            }
        } else {
            while *i < bytes.len() && is_arg_name_char(bytes[*i]) {
                *i += 1;
            }
        }

        let keyword = std::str::from_utf8(&bytes[kw_start..*i]).unwrap_or("");
        if keyword.is_empty() {
            return Err(IcuError {
                kind: IcuErrorKind::ExpectedBraceOrKeyword,
                position: *i,
            });
        }

        // Validate keyword
        if !keyword.starts_with('=') && !PLURAL_KEYWORDS.contains(&keyword) {
            return Err(IcuError {
                kind: IcuErrorKind::InvalidPluralKeyword(keyword.to_string()),
                position: kw_start,
            });
        }

        if keyword == "other" {
            has_other = true;
        }

        skip_whitespace(bytes, i);

        // Expect opening brace for message
        if *i >= bytes.len() || bytes[*i] != b'{' {
            return Err(IcuError {
                kind: IcuErrorKind::ExpectedBraceOrKeyword,
                position: *i,
            });
        }
        *i += 1;

        // Parse nested message
        parse_message(bytes, i, 1)?;

        if *i >= bytes.len() || bytes[*i] != b'}' {
            return Err(IcuError {
                kind: IcuErrorKind::UnclosedBrace,
                position: start,
            });
        }
        *i += 1;
    }

    if !has_other {
        return Err(IcuError {
            kind: IcuErrorKind::MissingOtherCategory,
            position: start,
        });
    }

    Ok(())
}

fn parse_select(bytes: &[u8], i: &mut usize, start: usize) -> Result<(), IcuError> {
    if *i >= bytes.len() || bytes[*i] != b',' {
        return Err(IcuError {
            kind: IcuErrorKind::ExpectedComma,
            position: *i,
        });
    }
    *i += 1;
    skip_whitespace(bytes, i);

    let mut has_other = false;

    // Parse select cases: keyword {msg}
    loop {
        skip_whitespace(bytes, i);

        if *i >= bytes.len() {
            return Err(IcuError {
                kind: IcuErrorKind::UnclosedBrace,
                position: start,
            });
        }

        if bytes[*i] == b'}' {
            *i += 1;
            break;
        }

        // Read keyword
        let kw_start = *i;
        while *i < bytes.len() && is_arg_name_char(bytes[*i]) {
            *i += 1;
        }

        let keyword = std::str::from_utf8(&bytes[kw_start..*i]).unwrap_or("");
        if keyword.is_empty() {
            return Err(IcuError {
                kind: IcuErrorKind::InvalidSelectSyntax,
                position: *i,
            });
        }

        if keyword == "other" {
            has_other = true;
        }

        skip_whitespace(bytes, i);

        // Expect opening brace for message
        if *i >= bytes.len() || bytes[*i] != b'{' {
            return Err(IcuError {
                kind: IcuErrorKind::ExpectedBraceOrKeyword,
                position: *i,
            });
        }
        *i += 1;

        // Parse nested message
        parse_message(bytes, i, 1)?;

        if *i >= bytes.len() || bytes[*i] != b'}' {
            return Err(IcuError {
                kind: IcuErrorKind::UnclosedBrace,
                position: start,
            });
        }
        *i += 1;
    }

    if !has_other {
        return Err(IcuError {
            kind: IcuErrorKind::MissingOtherCategory,
            position: start,
        });
    }

    Ok(())
}

fn skip_whitespace(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn is_arg_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Extract all placeholder names from an ICU message.
/// Returns a sorted Vec of unique placeholder names.
pub fn extract_placeholders(input: &str) -> Vec<String> {
    let mut placeholders = std::collections::HashSet::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'{' {
            i += 1;
            // Skip whitespace
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            // Read argument name
            let start = i;
            while i < bytes.len() && is_arg_name_char(bytes[i]) {
                i += 1;
            }
            if i > start {
                if let Ok(name) = std::str::from_utf8(&bytes[start..i]) {
                    placeholders.insert(name.to_string());
                }
            }
            // Skip to closing brace (handle nesting)
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    b'\'' => {
                        i += 1;
                        while i < bytes.len() && bytes[i] != b'\'' {
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        } else if bytes[i] == b'\'' {
            // Skip quoted content
            i += 1;
            if i < bytes.len() && bytes[i] == b'\'' {
                i += 1;
            } else {
                while i < bytes.len() && bytes[i] != b'\'' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    let mut result: Vec<String> = placeholders.into_iter().collect();
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_variable() {
        assert!(parse("{name}").is_ok());
        assert!(parse("Hello {name}!").is_ok());
        assert!(parse("{a} and {b}").is_ok());
    }

    #[test]
    fn unclosed_brace() {
        let err = parse("{name").unwrap_err();
        assert_eq!(err.kind, IcuErrorKind::UnclosedBrace);
    }

    #[test]
    fn unmatched_close_brace() {
        let err = parse("hello}").unwrap_err();
        assert_eq!(err.kind, IcuErrorKind::UnmatchedCloseBrace);
    }

    #[test]
    fn empty_argument() {
        let err = parse("{}").unwrap_err();
        assert_eq!(err.kind, IcuErrorKind::EmptyArgument);
    }

    #[test]
    fn plural_basic() {
        assert!(parse("{count, plural, one {# item} other {# items}}").is_ok());
        assert!(parse("{n, plural, =0 {none} =1 {one} other {many}}").is_ok());
    }

    #[test]
    fn plural_all_keywords() {
        assert!(parse("{n, plural, zero {0} one {1} two {2} few {few} many {many} other {other}}").is_ok());
    }

    #[test]
    fn plural_missing_other() {
        let err = parse("{n, plural, one {one}}").unwrap_err();
        assert_eq!(err.kind, IcuErrorKind::MissingOtherCategory);
    }

    #[test]
    fn plural_invalid_keyword() {
        let err = parse("{n, plural, invalid {x} other {y}}").unwrap_err();
        assert!(matches!(err.kind, IcuErrorKind::InvalidPluralKeyword(_)));
    }

    #[test]
    fn select_basic() {
        assert!(parse("{gender, select, male {He} female {She} other {They}}").is_ok());
    }

    #[test]
    fn select_missing_other() {
        let err = parse("{g, select, male {He} female {She}}").unwrap_err();
        assert_eq!(err.kind, IcuErrorKind::MissingOtherCategory);
    }

    #[test]
    fn selectordinal() {
        assert!(parse("{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}").is_ok());
    }

    #[test]
    fn nested_plural_in_select() {
        let msg = "{gender, select, male {{count, plural, one {He has # item} other {He has # items}}} other {They have items}}";
        assert!(parse(msg).is_ok());
    }

    #[test]
    fn number_format() {
        assert!(parse("{amount, number}").is_ok());
        assert!(parse("{amount, number, currency}").is_ok());
    }

    #[test]
    fn date_format() {
        assert!(parse("{date, date}").is_ok());
        assert!(parse("{date, date, short}").is_ok());
    }

    #[test]
    fn quoted_text() {
        assert!(parse("'{' is a brace").is_ok());
        assert!(parse("It''s working").is_ok());
    }

    #[test]
    fn real_world_example() {
        let msg = "You have {count, plural, =0 {no messages} one {# message} other {# messages}} from {sender}.";
        assert!(parse(msg).is_ok());
    }

    // extract_placeholders tests

    #[test]
    fn extracts_simple_placeholders() {
        assert_eq!(extract_placeholders("{name}"), vec!["name"]);
        assert_eq!(extract_placeholders("Hello {name}!"), vec!["name"]);
    }

    #[test]
    fn extracts_multiple_placeholders() {
        let mut result = extract_placeholders("{a} and {b} and {c}");
        result.sort();
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn extracts_from_plural() {
        let result = extract_placeholders("{count, plural, one {# item} other {# items}}");
        assert_eq!(result, vec!["count"]);
    }

    #[test]
    fn extracts_from_nested() {
        let msg = "Hello {name}, you have {count, plural, one {# msg} other {# msgs}}";
        let mut result = extract_placeholders(msg);
        result.sort();
        assert_eq!(result, vec!["count", "name"]);
    }

    #[test]
    fn no_duplicates() {
        let result = extract_placeholders("{name} and {name} again");
        assert_eq!(result, vec!["name"]);
    }

    #[test]
    fn empty_for_plain_text() {
        assert!(extract_placeholders("Hello world").is_empty());
    }
}
