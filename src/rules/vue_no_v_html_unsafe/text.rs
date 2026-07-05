use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["v-html"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Blank `<!-- ... -->` comment spans before scanning so a `v-html`
        // substring inside commented-out markup (e.g. an `eslint-disable
        // vue/no-v-html` comment) is not matched. Masking preserves byte
        // length and newlines, so line/column offsets stay aligned.
        let masked = crate::rules::vue_template_helpers::mask_html_comments(ctx.source);
        let lines: Vec<&str> = masked.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let Some(col) = find_v_html_directive(line) else {
                continue;
            };
            if line.contains("sanitize(") || line.contains("DOMPurify") {
                continue;
            }
            let prev_has_sanitize =
                i > 0 && (lines[i - 1].contains("sanitize") || lines[i - 1].contains("// safe"));
            if !prev_has_sanitize {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "`v-html` without sanitization is an XSS risk. Wrap the value in `DOMPurify.sanitize(...)`.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diags
    }
}

/// Byte offset of a `v-html` *directive token* in `line`, or `None`.
///
/// Matches `v-html` only where it stands alone as an attribute name: preceded
/// by the tag/attribute boundary (`<`, whitespace, or line start) and followed
/// by `=`, `>`, `/`, whitespace, or line end. So `data-v-html`, `no-v-html`,
/// and a `v-html` substring joined to a neighbouring character are not matched.
fn find_v_html_directive(line: &str) -> Option<usize> {
    const TOKEN: &str = "v-html";
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(rel) = line[from..].find(TOKEN) {
        let start = from + rel;
        let end = start + TOKEN.len();
        let before_ok =
            start == 0 || bytes[start - 1] == b'<' || bytes[start - 1].is_ascii_whitespace();
        let after_ok = end >= bytes.len()
            || matches!(bytes[end], b'=' | b'>' | b'/')
            || bytes[end].is_ascii_whitespace();
        if before_ok && after_ok {
            return Some(start);
        }
        from = start + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }
    #[test]
    fn flags_v_html_no_sanitize() {
        assert_eq!(run("<div v-html=\"userContent\" />").len(), 1);
    }
    #[test]
    fn allows_v_html_with_sanitize() {
        assert!(run("<div v-html=\"DOMPurify.sanitize(content)\" />").is_empty());
    }

    #[test]
    fn ignores_v_html_inside_html_comment() {
        // Issue #7352 (vue-vben-admin workbench-trends.vue): the `v-html`
        // substring lives only inside an `<!-- eslint-disable vue/no-v-html -->`
        // comment, which renders nothing — it must not be flagged.
        let source = "<template>\n  <!-- eslint-disable vue/no-v-html -->\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_data_v_html_attribute() {
        // `data-v-html` is a distinct attribute name, not a `v-html` directive:
        // the token boundary rejects the `v-html` substring inside it.
        assert!(run("<div data-v-html=\"x\"></div>").is_empty());
    }

    #[test]
    fn flags_real_v_html_directive() {
        // Control: a genuine unsanitized `v-html` directive still flags.
        assert_eq!(run("<p v-html=\"item.content\"></p>").len(), 1);
    }

    #[test]
    fn comment_and_directive_yields_exactly_one_diagnostic() {
        // Issue #7352: an `eslint-disable vue/no-v-html` comment sitting above a
        // real `v-html` directive must yield exactly one diagnostic, on the
        // directive line (5), not a spurious second one on the comment (2).
        let source = "<template>\n  <!-- eslint-disable vue/no-v-html -->\n  <p\n    class=\"mt-1 truncate text-xs/5\"\n    v-html=\"item.content\"\n  ></p>\n</template>";
        let diags = run(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }
}
