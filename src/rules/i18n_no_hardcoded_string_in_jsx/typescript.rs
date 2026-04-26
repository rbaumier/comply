use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_text"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("").trim();
    if text.is_empty() || !text.contains(' ') || text.len() <= 2 { return; }
    if text.chars().all(|c| c.is_ascii_digit() || c.is_ascii_punctuation()) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Hardcoded string \"{text}\" in JSX — wrap with `t()`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_text_content() {
        assert_eq!(run("<div>Hello World</div>").len(), 1);
    }
    #[test]
    fn flags_paragraph() {
        assert_eq!(run("<p>Submit your application</p>").len(), 1);
    }
    #[test]
    fn allows_translation_call() {
        assert!(run("<div>{t('home.greeting')}</div>").is_empty());
    }
    #[test]
    fn allows_whitespace_only() {
        assert!(run("<div> </div>").is_empty());
    }
    #[test]
    fn allows_single_char() {
        assert!(run("<span>:</span>").is_empty());
    }
}
