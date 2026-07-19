//! vue-no-comment-textnodes — Vue text backend.
//!
//! Flags accidental text comments (// or /* */) inside Vue template elements.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::{
    extract_template, is_vue_file, mask_html_comments, template_lang,
};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        // The premise — a JS `//` comment at a text-node position renders as
        // visible text — holds only for the default HTML template grammar. A
        // `<template lang="pug">` (or jade/haml/…) is preprocessed: `//-` is a
        // valid silent comment and there are no `<`/`>` tag boundaries, so the
        // whole HTML text-node scan would be false. Skip any non-HTML lang.
        if template_lang(ctx.source).is_some_and(|lang| !lang.eq_ignore_ascii_case("html")) {
            return Vec::new();
        }
        let Some(template) = extract_template(ctx.source) else {
            return Vec::new();
        };
        let template_offset = template.as_ptr() as usize - ctx.source.as_ptr() as usize;
        let lines_before = ctx.source[..template_offset].matches('\n').count();

        // Blank out `<!-- -->` comments so their content can neither desync the
        // region scan below nor be mistaken for a JS text-node comment.
        let scanned = mask_html_comments(template);

        let mut diagnostics = Vec::new();
        // Running expression state carried across template lines. A `//` or `/* */`
        // comment renders as a template text node only when it sits outside every
        // JS-expression region: a quoted attribute value (`@click="() => { … }"`,
        // `:style="{ … }"`), a `{{ … }}` interpolation, and a block comment.
        let mut state = ExprState::default();

        for (i, line) in scanned.lines().enumerate() {
            let trimmed = line.trim_start();
            if !state.in_expression()
                && (trimmed.starts_with("//") || trimmed.starts_with("/*"))
                && !trimmed.starts_with("///")
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: lines_before + 1 + i,
                    column: 1,
                    rule_id: "vue-no-comment-textnodes".into(),
                    message: "JS comment syntax in template renders as text — use `<!-- -->`."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            state.advance(line);
        }
        diagnostics
    }
}

/// Running lexer state used to tell whether a template position sits inside a JS
/// binding/interpolation expression (a real comment) or at a bare text node.
///
/// Quotes and brackets only carry meaning inside expression regions, so they are
/// tracked as regions rather than counted globally: a `'` or `(` in rendered text
/// (`<p>Don't</p>`, `<p>(note)</p>`) stays inert and cannot desync the scan.
#[derive(Default)]
struct ExprState {
    /// The opening quote byte of the attribute-value/string region currently
    /// open, or `None` outside any string. Only opened while inside a tag.
    in_string: Option<u8>,
    /// Whether a `/* … */` block comment is currently open.
    in_block_comment: bool,
    /// Whether the scan is between a tag's `<` and its closing `>`, where
    /// attribute values (the quoted JS binding expressions) live.
    in_tag: bool,
    /// Whether a `{{ … }}` interpolation is currently open.
    in_mustache: bool,
}

impl ExprState {
    /// True when the current position is inside a JS-expression region, so a
    /// comment there is real JS rather than a rendered text node.
    fn in_expression(&self) -> bool {
        self.in_string.is_some() || self.in_block_comment || self.in_mustache
    }

    /// Advance the state across one template line. `<`/`>` open and close a tag;
    /// inside a tag `"`/`'`/`` ` `` open an attribute-value string that ends on the
    /// matching quote (honouring `\` escapes) and can span lines. In text content
    /// `{{`/`}}` bound an interpolation. A `//` runs to end of line; a `/* */`
    /// block comment persists across lines. Characters inside a string, comment,
    /// or interpolation are otherwise inert, so brackets/quotes there never shift
    /// the region state.
    fn advance(&mut self, line: &str) {
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            let b = bytes[i];
            if self.in_block_comment {
                if b == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    self.in_block_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if let Some(quote) = self.in_string {
                if b == b'\\' {
                    i += 2; // skip the escaped character
                    continue;
                }
                if b == quote {
                    self.in_string = None;
                }
                i += 1;
                continue;
            }
            if self.in_mustache {
                if b == b'}' && i + 1 < len && bytes[i + 1] == b'}' {
                    self.in_mustache = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if self.in_tag {
                match b {
                    b'"' | b'\'' | b'`' => self.in_string = Some(b),
                    b'>' => self.in_tag = false,
                    _ => {}
                }
                i += 1;
                continue;
            }
            // Text-node content: only a tag, an interpolation, or a comment opens
            // a region; loose quotes and single brackets are inert text.
            match b {
                b'<' => self.in_tag = true,
                b'{' if i + 1 < len && bytes[i + 1] == b'{' => {
                    self.in_mustache = true;
                    i += 2;
                    continue;
                }
                b'/' if i + 1 < len && bytes[i + 1] == b'/' => return, // line comment to EOL
                b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                    self.in_block_comment = true;
                    i += 2;
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("c.vue"), source))
    }

    #[test]
    fn flags_js_comment_in_template() {
        let src = "<template>\n  <div>\n    // this is a comment\n  </div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_html_comment() {
        let src = "<template>\n  <!-- this is fine -->\n  <div></div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let d = Check.check(&CheckCtx::for_test(
            Path::new("f.ts"),
            "// comment in template",
        ));
        assert!(d.is_empty());
    }

    #[test]
    fn allows_comment_in_event_handler_arrow_body() {
        let src = "<template>\n  <button @mousedown=\"(e) => {\n    // only left button\n    handle();\n  }\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_comment_in_object_binding() {
        let src = "<template>\n  <div :style=\"{\n    // prevent interaction\n    pointerEvents: 'none',\n  }\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_comment_after_trailing_comma_in_call() {
        let src = "<template>\n  <div :class=\"cn(\n    base,\n    // Selected\n    'x',\n  )\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_block_comment_text_node_after_markup() {
        let src = "<template>\n  <span>hi</span>\n  /* stray */\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    // --- #7203 regression: comments inside multi-line JS binding expressions ---

    #[test]
    fn allows_multiline_comment_block_in_handler() {
        // Three consecutive `//` lines inside an `@interact-outside` handler body:
        // none render as text, so none are flagged.
        let src = "<template>\n  <DismissableLayer @interact-outside=\"(event) => {\n    if (!event.defaultPrevented) {\n      hasInteractedOutsideRef = true;\n    }\n    // Prevent dismissing when clicking the trigger.\n    // As the trigger is already setup to close.\n    // cause it to close and immediately open.\n    const target = event.target;\n  }\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_comment_after_statement_in_handler() {
        // A single `//` comment whose previous line ends with `;` (a statement),
        // still inside the quoted handler expression, is not a text node.
        let src = "<template>\n  <Foo @close-auto-focus=\"(event) => {\n    if (!hasInteractedOutsideRef) rootContext.triggerElement.value?.focus();\n    // Always prevent auto focus.\n    event.preventDefault();\n  }\" />\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_text_node_comment_between_elements() {
        // A genuine JS-style comment sitting as a bare text node between two
        // elements is still reported — the rule's real target.
        let src =
            "<template>\n  <span>a</span>\n  // stray text node\n  <span>b</span>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn plain_string_attr_brackets_do_not_hide_text_node_comment() {
        // Brackets inside a quoted attribute value (`class=\"a[b]\"`) stay inside
        // the string region, so the following bare text-node comment is still
        // flagged.
        let src =
            "<template>\n  <div class=\"a[b]\"></div>\n  // genuine text node\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn apostrophe_in_text_does_not_hide_following_text_node_comment() {
        // An apostrophe in rendered text is inert (not an attribute-value quote),
        // so it must not swallow a genuine text-node comment on a later line.
        let src = "<template>\n  <p>Don't click</p>\n  // stray text node\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_comment_in_multiline_interpolation() {
        // A comment inside a multi-line `{{ … }}` interpolation is real JS.
        let src = "<template>\n  <div>{{\n    // computed label\n    label\n  }}</div>\n</template>";
        assert!(run(src).is_empty());
    }

    // --- #7704 regression: preprocessor templates (`<template lang="pug">`) ---

    #[test]
    fn skips_pug_template_silent_comments() {
        // In a `lang="pug"` template, `//-` is Pug's silent-comment syntax, not a
        // JS text-node comment. The HTML text-node premise does not apply, so no
        // `//-` line is flagged.
        let src = "<template lang=\"pug\">\ndiv(:class=\"$style.bg\")\n//- div(:class=\"$style.bg\" :style=\"bgStyle\")\n//- button(type=\"button\" @click=\"max\")\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_jade_template_silent_comments() {
        // `jade` is Pug's former name — the same preprocessor, so the gate skips
        // it too.
        let src = "<template lang=\"jade\">\ndiv\n//- silent comment\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_explicit_html_lang_template() {
        // `lang="html"` is the default grammar, so a genuine text-node comment is
        // still reported.
        let src = "<template lang=\"html\">\n  <div>\n    // this is a comment\n  </div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_uppercase_pug_lang() {
        // The lang comparison is case-insensitive: `PUG` is still a preprocessor.
        let src = "<template lang=\"PUG\">\ndiv\n//- silent\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_uppercase_html_lang() {
        // `HTML` folds to the default grammar, so a text-node comment still fires.
        let src = "<template lang=\"HTML\">\n  <div>\n    // this is a comment\n  </div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }
}
