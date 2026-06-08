use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const ZWJ: char = '\u{200D}';

fn has_misleading_char_class(pattern: &str) -> bool {
    let mut in_class = false;
    let mut escaped = false;
    for ch in pattern.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '[' && !in_class {
            in_class = true;
            continue;
        }
        if ch == ']' && in_class {
            in_class = false;
            continue;
        }
        if in_class && (ch as u32 > 0xFFFF || ch == ZWJ) {
            return true;
        }
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        if !has_misleading_char_class(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Character class contains multi-codepoint graphemes \u{2014} they will be split into individual code points.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_emoji_in_char_class() {
        // U+1F468 is above U+FFFF
        let code = "const re = /[\u{1F468}]/;";
        assert_eq!(run_on(code).len(), 1);
    }


    #[test]
    fn flags_zwj_in_char_class() {
        // Family emoji with ZWJ
        let code = "const re = /[\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}]/;";
        assert_eq!(run_on(code).len(), 1);
    }


    #[test]
    fn allows_ascii_char_class() {
        assert!(run_on("const re = /[abc]/;").is_empty());
    }


    #[test]
    fn allows_emoji_outside_char_class() {
        assert!(run_on("const re = /\u{1F468}/;").is_empty());
    }


    #[test]
    fn ignores_tailwind_class_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr] [\u{1F468}]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "https://example.com/[\u{1F468}]/path";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@scope/[\u{1F468}]-pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
