//! regex-use-unicode-flag OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::RegExpFlags;
use std::sync::Arc;

pub struct Check;

/// Returns true if the regex pattern contains a `\p{...}` or `\P{...}`
/// Unicode property escape (respecting backslash escaping).
fn has_unicode_property_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if (next == b'p' || next == b'P') && i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                return true;
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    false
}

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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        let pattern = re.regex.pattern.text.as_str();
        if !has_unicode_property_escape(pattern) {
            return;
        }
        if re.regex.flags.contains(RegExpFlags::U) || re.regex.flags.contains(RegExpFlags::V) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unicode property escape (`\\p{...}`) requires the `u` or `v` flag.".into(),
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
    fn flags_unicode_prop_without_u() {
        let diags = run_on(r#"const re = /\p{Letter}/;"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_uppercase_p_without_u() {
        let diags = run_on(r#"const re = /\P{Number}/i;"#);
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_unicode_prop_with_u() {
        assert!(run_on(r#"const re = /\p{Letter}/u;"#).is_empty());
    }


    #[test]
    fn allows_unicode_prop_with_v() {
        assert!(run_on(r#"const re = /\p{Letter}/v;"#).is_empty());
    }


    #[test]
    fn allows_regex_without_unicode_escape() {
        assert!(run_on(r#"const re = /abc/;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_class_in_string() {
        let src = r#"const x = "has-[\p{foo}]:grid";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/\\p{Letter}/path";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_with_empty_flags() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
