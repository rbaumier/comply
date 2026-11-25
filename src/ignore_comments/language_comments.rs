//! The bytes of a file its own language comments out.
//!
//! A comply marker counts only there.
//! In doubt, a byte is no comment.
//! The marker then suppresses nothing.

use crate::files::Language;
use std::path::Path;

/// Marks each byte of `source` its language comments out.
/// `None` for a language read line by line, as before.
pub fn bytes(path: &Path, source: &str) -> Option<Vec<bool>> {
    let language = crate::files::language_of(path)?;
    match language {
        Language::Sql => {
            use crate::rules::sql_helpers::{DollarBody, sql_comment_bytes};
            Some(sql_comment_bytes(source, DollarBody::Routine))
        }
        Language::Toml => Some(toml_comment_bytes(source)),
        Language::Nginx => Some(nginx_comment_bytes(source)),
        Language::Css => Some(css_comment_bytes(source)),
        Language::Vue => Some(vue_comment_bytes(source)),
        Language::TypeScript
        | Language::Tsx
        | Language::JavaScript
        | Language::Rust
        | Language::Yaml
        | Language::Dockerfile => Some(grammar_comment_bytes(language, source)),
        Language::Json | Language::Markdown | Language::Html => None,
    }
}

/// The bytes a comment node of the grammar spans.
fn grammar_comment_bytes(language: Language, source: &str) -> Vec<bool> {
    let mut comment = vec![false; source.len()];
    let mut parser = tree_sitter::Parser::new();
    let Some(tree) = crate::parsing::parse_with_grammar(&mut parser, language, source.as_bytes())
    else {
        return comment;
    };
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let is_comment = node.kind().ends_with("comment");
        if is_comment && let Some(bytes) = comment.get_mut(node.byte_range()) {
            bytes.fill(true);
        }
        if !is_comment && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return comment;
            }
        }
    }
}

/// A Vue script is read by its own grammar.
/// The rest is read line by line, as before.
fn vue_comment_bytes(source: &str) -> Vec<bool> {
    let mut comment = vec![true; source.len()];
    let mut parser = tree_sitter::Parser::new();
    let Some(tree) =
        crate::parsing::parse_with_grammar(&mut parser, Language::Vue, source.as_bytes())
    else {
        return comment;
    };
    for script in crate::rules::vue_sfc::extract_scripts(&tree, source) {
        let language = match script.lang {
            Some("tsx" | "jsx") => Language::Tsx,
            _ => Language::TypeScript,
        };
        let start = source
            .split_inclusive('\n')
            .take(script.start_row)
            .map(str::len)
            .sum::<usize>()
            .saturating_add(script.start_column);
        let end = start.saturating_add(script.text.len());
        if let Some(bytes) = comment.get_mut(start..end) {
            bytes.copy_from_slice(&grammar_comment_bytes(language, script.text));
        }
    }
    comment
}

/// TOML: a `#` outside a string opens a comment.
/// It ends with its line.
fn toml_comment_bytes(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut comment = vec![false; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        i = match bytes[i] {
            b'#' => mark_to_line_end(bytes, i, &mut comment),
            b'"' | b'\'' => toml_string_end(bytes, i),
            _ => i.saturating_add(1),
        };
    }
    comment
}

/// Where the TOML string opening at `open` ends.
/// A multi-line one may never end.
fn toml_string_end(bytes: &[u8], open: usize) -> usize {
    let quote = bytes[open];
    let triple = [quote; 3];
    let is_multi_line = bytes[open..].starts_with(&triple);
    let closer = if is_multi_line {
        &triple[..]
    } else {
        &triple[..1]
    };
    let mut i = open.saturating_add(closer.len());
    while i < bytes.len() {
        if quote == b'"' && bytes[i] == b'\\' {
            i = i.saturating_add(2);
            continue;
        }
        if bytes[i..].starts_with(closer) {
            let end = i.saturating_add(closer.len());
            // `"""a""""` holds `a"`: quotes may end the content.
            let content_quotes = if is_multi_line {
                bytes[end..]
                    .iter()
                    .take(2)
                    .take_while(|byte| **byte == quote)
                    .count()
            } else {
                0
            };
            return end.saturating_add(content_quotes);
        }
        if !is_multi_line && bytes[i] == b'\n' {
            return i;
        }
        i = i.saturating_add(1);
    }
    bytes.len()
}

/// nginx: a `#` opens a comment where a token starts.
/// Glued to a word, as in `a#b`, it is text.
fn nginx_comment_bytes(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut comment = vec![false; bytes.len()];
    let mut is_token_start = true;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        i = match byte {
            b'#' if is_token_start => mark_to_line_end(bytes, i, &mut comment),
            b'"' | b'\'' if is_token_start => quote_end(bytes, i, true),
            b'\\' => i.saturating_add(2),
            _ => i.saturating_add(1),
        };
        is_token_start = byte.is_ascii_whitespace() || matches!(byte, b';' | b'{' | b'}');
    }
    comment
}

/// CSS: a `/*` outside a string opens a comment.
/// The grammar misreads a string continued by `\`.
fn css_comment_bytes(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut comment = vec![false; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        i = match bytes[i] {
            b'/' if bytes[i..].starts_with(b"/*") => {
                let body = i.saturating_add(2);
                let end = bytes[body..]
                    .windows(2)
                    .position(|pair| pair == b"*/")
                    .map_or(bytes.len(), |offset| {
                        body.saturating_add(offset).saturating_add(2)
                    });
                comment[i..end].fill(true);
                end
            }
            b'"' | b'\'' => quote_end(bytes, i, false),
            b'\\' => i.saturating_add(2),
            _ => i.saturating_add(1),
        };
    }
    comment
}

/// Where the quoted token opening at `open` ends.
/// Unless it can span lines, an unescaped newline ends it.
fn quote_end(bytes: &[u8], open: usize, can_span_lines: bool) -> usize {
    let quote = bytes[open];
    let mut i = open.saturating_add(1);
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i = i.saturating_add(2),
            b'\n' if !can_span_lines => return i,
            byte if byte == quote => return i.saturating_add(1),
            _ => i = i.saturating_add(1),
        }
    }
    bytes.len()
}

/// Marks the line comment opening at `start`.
/// Returns where it ends.
fn mark_to_line_end(bytes: &[u8], start: usize, comment: &mut [bool]) -> usize {
    let end = bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start.saturating_add(offset));
    comment[start..end].fill(true);
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the language of `file` comments out in `source`.
    fn comments(file: &str, source: &str) -> String {
        let comment = bytes(Path::new(file), source).expect("a comment reading");
        source
            .char_indices()
            .filter(|&(i, _)| comment[i])
            .map(|(_, c)| c)
            .collect()
    }

    #[test]
    fn yaml_comments_are_neither_block_scalars_nor_glued_hashes() {
        let yaml = "a: |\n  # in block\nb: c# glued\nd: \"# quoted\" # real\n";
        assert_eq!(comments("ci.yml", yaml), "# real");
    }

    #[test]
    fn toml_comments_skip_every_string_form() {
        let toml = "a = \"\"\"\n# in basic\n\"\"\" # one\nb = '''\n# in literal\n''' # two\n\
                    c = \"\\\" # escaped\" # three\nd = 'x' # four\ne = \"\"\"x\"\"\"\" # five\n";
        assert_eq!(
            comments("Cargo.toml", toml),
            "# one# two# three# four# five"
        );
    }

    #[test]
    fn toml_string_left_open_holds_no_comment() {
        assert_eq!(comments("Cargo.toml", "a = \"\"\"\n# never closed\n"), "");
    }

    #[test]
    fn nginx_comment_opens_a_token_outside_quotes() {
        let conf = "a b#c;# one\nd \"e\n# quoted\" # two\nf\\#g # three\n";
        assert_eq!(comments("nginx.conf", conf), "# one# two# three");
    }

    #[test]
    fn rust_comment_is_no_raw_string() {
        let rust = "const A: &str = r#\"\n// in string\n\"#; // real\n/* block */\n";
        assert_eq!(comments("lib.rs", rust), "// real/* block */");
    }

    #[test]
    fn typescript_comment_is_no_template_literal() {
        let ts = "const a = `\n// in template\n`; // real\n";
        assert_eq!(comments("a.ts", ts), "// real");
    }

    #[test]
    fn css_comment_is_no_string() {
        let css = ".a { content: \"x\\\n/* in string */\"; } /* real */\n\
                   .b { content: '\n/* after a broken string */\n";
        assert_eq!(
            comments("a.css", css),
            "/* real *//* after a broken string */"
        );
    }

    #[test]
    fn dockerfile_comment_is_no_heredoc_line() {
        let dockerfile = "FROM alpine\n# real\nCOPY <<EOF /motd\n# in heredoc\nEOF\n";
        assert_eq!(comments("Dockerfile", dockerfile), "# real");
    }

    #[test]
    fn vue_script_is_read_by_its_grammar_and_the_rest_line_by_line() {
        let vue =
            "<script setup lang=\"ts\">\nconst a = `\n// in template\n`; // real\n</script>\n";
        let comment = bytes(Path::new("A.vue"), vue).expect("a comment reading");
        let at = |text: &str| comment[vue.find(text).expect("text in the file")];
        assert!(!at("// in template"));
        assert!(at("// real"));
        assert!(at("<script"));
    }

    #[test]
    fn json_is_read_line_by_line() {
        assert!(bytes(Path::new("a.json"), "{}").is_none());
    }
}
