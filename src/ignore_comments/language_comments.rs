//! The bytes of a file its own language comments out.
//!
//! A comply marker counts only there.
//! In doubt, a byte is no comment.
//! The marker then suppresses nothing.

use crate::files::Language;
use crate::rules::sql_helpers::is_ident_byte;
use crate::rules::vue_sfc::{tag_close_offset, tag_lang};
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
        Language::Dockerfile => Some(dockerfile_comment_bytes(source)),
        // Only the TSX grammar reads JSX.
        Language::JavaScript => Some(grammar_comment_bytes(Language::Tsx, source)),
        Language::TypeScript | Language::Tsx | Language::Rust | Language::Yaml => {
            Some(grammar_comment_bytes(language, source))
        }
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

/// Dockerfile: a comment holds its whole line.
/// After an instruction, a `#` belongs to its argument.
fn dockerfile_comment_bytes(source: &str) -> Vec<bool> {
    let mut comment = grammar_comment_bytes(Language::Dockerfile, source);
    let mut start = 0usize;
    for line in source.split_inclusive('\n') {
        let end = start.saturating_add(line.len());
        if !line.trim_start().starts_with('#') {
            comment[start..end].fill(false);
        }
        start = end;
    }
    comment
}

/// Elements whose content HTML reads as text.
const TEXT_ELEMENTS: [&str; 6] = ["iframe", "noscript", "script", "style", "textarea", "title"];

/// Vue: `<!-- … -->` outside tags, mustaches and text elements.
/// A script is read by its grammar.
/// A style is read as CSS.
/// Any other block is text.
fn vue_comment_bytes(source: &str) -> Vec<bool> {
    let mut comment = vec![false; source.len()];
    // The `<template>` elements still open.
    let mut depth = 0usize;
    let mut rest = source;
    while !rest.is_empty() {
        let at = source.len().saturating_sub(rest.len());
        let is_block = depth == 0;
        let len = if rest.starts_with("<!--") {
            let len = closed_at(rest, "-->").unwrap_or_default();
            comment[at..at.saturating_add(len)].fill(true);
            len
        } else if !is_block && rest.starts_with("{{") {
            closed_at(rest, "}}").unwrap_or_default()
        } else if let Some(tag) = VueTag::opening(rest, is_block) {
            let start = at.saturating_add(tag.tag.len());
            let read = tag.text_comment_bytes();
            comment[start..start.saturating_add(read.len())].copy_from_slice(&read);
            depth = depth.saturating_add(usize::from(tag.opens_template()));
            tag.tag.len().saturating_add(tag.text.len())
        } else {
            depth = depth.saturating_sub(usize::from(rest.starts_with("</template")));
            let next = rest.char_indices().skip(1);
            next.filter(|(_, c)| matches!(c, '<' | '{'))
                .map(|(next, _)| next)
                .next()
                .unwrap_or(rest.len())
        };
        // What never closes holds no comment.
        rest = if len == 0 { "" } else { &rest[len..] };
    }
    comment
}

/// An opening tag of a Vue file.
struct VueTag<'s> {
    name: &'s str,
    /// The tag, its `>` included.
    tag: &'s str,
    /// The content HTML reads as text, empty for markup.
    text: &'s str,
    /// Whether no `<template>` holds it.
    is_block: bool,
}

impl<'s> VueTag<'s> {
    /// The tag `rest` opens with.
    fn opening(rest: &'s str, is_block: bool) -> Option<Self> {
        let name = tag_name(rest)?;
        let len = tag_close_offset(rest.as_bytes(), 0)
            .map_or(rest.len(), |close| close.saturating_add(1));
        let (tag, content) = rest.split_at(len);
        let holds_text = if name == "template" {
            is_block && tag_lang(tag).is_some_and(|lang| lang != "html")
        } else {
            is_block || TEXT_ELEMENTS.contains(&name)
        };
        let text = if holds_text && !tag.ends_with("/>") {
            &content[..element_end(content, name)]
        } else {
            ""
        };
        Some(Self {
            name,
            tag,
            text,
            is_block,
        })
    }

    /// Whether markup follows, up to a `</template>`.
    fn opens_template(&self) -> bool {
        self.name == "template" && self.text.is_empty() && !self.tag.ends_with("/>")
    }

    /// The bytes of `text` its own language comments out.
    /// Empty when that text has none.
    fn text_comment_bytes(&self) -> Vec<bool> {
        match self.name {
            "script" if self.is_block => {
                let is_ts = tag_lang(self.tag) == Some("ts");
                let language = if is_ts {
                    Language::TypeScript
                } else {
                    Language::Tsx
                };
                grammar_comment_bytes(language, self.text)
            }
            "style" if self.is_block => css_comment_bytes(self.text),
            _ => Vec::new(),
        }
    }
}

/// Where the text opening with two bytes or more closes.
fn closed_at(text: &str, closer: &str) -> Option<usize> {
    let after = text.get(2..)?.find(closer)?;
    Some(after.saturating_add(2).saturating_add(closer.len()))
}

/// The name of the tag `text` opens with.
fn tag_name(text: &str) -> Option<&str> {
    let name = text.strip_prefix('<')?;
    let end = name.find(|c: char| c.is_whitespace() || matches!(c, '/' | '>'));
    let name = &name[..end.unwrap_or(name.len())];
    name.starts_with(|c: char| c.is_ascii_alphabetic())
        .then_some(name)
}

/// Where the closing tag of `name` opens in `content`.
fn element_end(content: &str, name: &str) -> usize {
    let closer = format!("</{name}");
    let bytes = content.as_bytes();
    bytes
        .windows(closer.len())
        .enumerate()
        .find(|(at, window)| {
            let next = bytes.get(at.saturating_add(closer.len()));
            window.eq_ignore_ascii_case(closer.as_bytes())
                && next.is_none_or(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>'))
        })
        .map_or(content.len(), |(at, _)| at)
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
/// A Lua block holds no nginx comment.
fn nginx_comment_bytes(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut comment = vec![false; bytes.len()];
    let mut is_lua_next = false;
    let mut i = 0;
    while i < bytes.len() {
        i = match bytes[i] {
            b'#' => mark_to_line_end(bytes, i, &mut comment),
            b'{' if is_lua_next => {
                is_lua_next = false;
                lua_block_end(bytes, i)
            }
            b';' | b'{' | b'}' => {
                is_lua_next = false;
                i.saturating_add(1)
            }
            byte if byte.is_ascii_whitespace() => i.saturating_add(1),
            _ => {
                let end = nginx_token_end(bytes, i);
                is_lua_next |= bytes[i..end].ends_with(b"_by_lua_block");
                end
            }
        };
    }
    comment
}

/// Where the nginx token starting at `start` ends.
/// The `{` of `${name}` belongs to it, and so does a `}`.
fn nginx_token_end(bytes: &[u8], start: usize) -> usize {
    let mut i = match bytes[start] {
        b'"' | b'\'' => quote_end(bytes, start, true),
        _ => start,
    };
    while i < bytes.len() {
        i = match bytes[i] {
            b'$' if bytes.get(i.saturating_add(1)) == Some(&b'{') => i.saturating_add(2),
            b'\\' => i.saturating_add(2),
            byte if byte.is_ascii_whitespace() || matches!(byte, b';' | b'{') => return i,
            _ => i.saturating_add(1),
        };
    }
    bytes.len()
}

/// Where the Lua block opening at `open` ends.
/// Braces in Lua strings and comments do not count.
fn lua_block_end(bytes: &[u8], open: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let next = i.saturating_add(1);
        i = match bytes[i] {
            b'{' => {
                depth = depth.saturating_add(1);
                next
            }
            b'}' if depth <= 1 => return next,
            b'}' => {
                depth = depth.saturating_sub(1);
                next
            }
            b'"' | b'\'' => quote_end(bytes, i, false),
            b'[' => lua_bracket_end(bytes, i).unwrap_or(next),
            b'-' if bytes[i..].starts_with(b"--") => {
                lua_bracket_end(bytes, i.saturating_add(2)).unwrap_or_else(|| line_end(bytes, i))
            }
            _ => next,
        };
    }
    bytes.len()
}

/// Where the Lua long bracket `[==[ … ]==]` opening at `open` ends.
fn lua_bracket_end(bytes: &[u8], open: usize) -> Option<usize> {
    let after = bytes.get(open..)?.strip_prefix(b"[")?;
    let level = after.iter().take_while(|byte| **byte == b'=').count();
    let body = after.get(level..)?.strip_prefix(b"[")?;
    let closer = [&b"]"[..], &b"=".repeat(level), b"]"].concat();
    let len = body
        .windows(closer.len())
        .position(|window| window == closer);
    let opened = bytes.len().saturating_sub(body.len());
    Some(len.map_or(bytes.len(), |len| {
        opened.saturating_add(len).saturating_add(closer.len())
    }))
}

/// CSS: a `/*` opens a comment outside a string.
/// A bare `url(…)` holds none.
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
            b'(' => bare_url_end(bytes, i),
            _ => i.saturating_add(1),
        };
    }
    comment
}

/// Where the `url(…)` whose bracket opens at `open` ends.
/// One that holds a quoted string ends at its bracket.
/// So does any other function.
fn bare_url_end(bytes: &[u8], open: usize) -> usize {
    let opened = open.saturating_add(1);
    let name = bytes[..open]
        .split(|byte| !is_ident_byte(*byte))
        .next_back();
    let first = bytes[opened..]
        .iter()
        .find(|byte| !byte.is_ascii_whitespace());
    let is_bare = !matches!(first, Some(b'"' | b'\''));
    if !is_bare || !name.is_some_and(|name| name.eq_ignore_ascii_case(b"url")) {
        return opened;
    }
    let mut i = opened;
    while i < bytes.len() {
        i = match bytes[i] {
            b'\\' => i.saturating_add(2),
            b')' => return i,
            _ => i.saturating_add(1),
        };
    }
    bytes.len()
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
    let end = line_end(bytes, start);
    comment[start..end].fill(true);
    end
}

/// Where the line holding `start` ends.
fn line_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start.saturating_add(offset))
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
    fn dockerfile_comment_holds_its_whole_line() {
        let dockerfile = "FROM alpine # after\nENV A=b # after\nRUN a \\\n  # real\n  b\n";
        assert_eq!(comments("Dockerfile", dockerfile).trim(), "# real");
    }

    #[test]
    fn css_comment_is_no_bare_url() {
        let css = ".a { b: url( /* bare */ ) url(\\)/* bare */) } /* one */\n\
                   .c { d: URL(\"x\" /* two */) calc(1px /* three */) }\n";
        assert_eq!(comments("a.css", css), "/* one *//* two *//* three */");
    }

    #[test]
    fn nginx_word_keeps_its_braces() {
        let conf = "a ${b}# c;\nd ^/(e{2}# f;\ng {# one\n}# two\n";
        assert_eq!(comments("nginx.conf", conf), "# one# two");
    }

    #[test]
    fn nginx_lua_block_holds_no_comment() {
        let conf = "a_by_lua_block $b {\n s = '}' .. \"}\" -- }\n t = [=[ } ]] ]=] # t\n \
                    --[[ }\n } ]] u = { # t }\n # in lua\n}\n# one\nc { # two\n}\n";
        assert_eq!(comments("nginx.conf", conf), "# one# two");
    }

    #[test]
    fn vue_comment_is_outside_tags_and_text() {
        let vue = "<!-- one -->\n<template>\n<template v-if=\"a\"><!-- two --></template>\n\
                   <p a=\"<!-- no -->\">{{ '<!-- no -->' }}<!----><!-- three --></p>\n\
                   <textarea><!-- no --></textarea><Textarea><!-- four --></Textarea>\n\
                   </template>\n<docs><!-- no --></docsy><!-- no --></docs>\n\
                   <script src=\"a\" />\n<!-- five -->\n<docs><!-- never closed -->\n";
        assert_eq!(
            comments("A.vue", vue),
            "<!-- one --><!-- two --><!----><!-- three --><!-- four --><!-- five -->"
        );
    }

    #[test]
    fn vue_script_and_style_are_read_by_their_language() {
        let vue = "<script setup lang=\"ts\">\nconst a = <T>`\n// no\n`; // one\n</script>\n\
                   <script>\nconst b = <p>\n// no\n</p>; // two\n</script>\n\
                   <style>\n.a { b: \"/* no */\" } /* three */\n// no\n</style>\n\
                   <template lang=\"pug\">\n// no\n<!-- no -->\n</template>\n<!-- four -->\n";
        assert_eq!(
            comments("A.vue", vue),
            "// one// two/* three */<!-- four -->"
        );
    }

    #[test]
    fn vue_text_that_never_closes_holds_no_comment() {
        for open in ["<!-- no", "<template>{{ no", "<template><p a=\"no"] {
            assert_eq!(
                comments("A.vue", &format!("{open}\n<!-- no --")),
                "",
                "{open}"
            );
        }
    }

    #[test]
    fn javascript_comment_is_no_jsx_text() {
        let js = "const a = <p>\n// text\n</p>; // real\n";
        assert_eq!(comments("a.js", js), "// real");
    }

    #[test]
    fn json_is_read_line_by_line() {
        assert!(bytes(Path::new("a.json"), "{}").is_none());
    }
}
