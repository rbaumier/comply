use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn contains_ecb(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-ecb") {
        return true;
    }
    if lower.contains(".ecb") {
        return true;
    }
    if lower == "ecb" {
        return true;
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else {
            return;
        };
        if !contains_ecb(lit.value.as_str()) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "ECB cipher mode is insecure — use CBC, CTR, or GCM instead.".into(),
            severity: super::META.severity,
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
    fn flags_aes_ecb() {
        assert_eq!(run_on("createCipheriv('aes-128-ecb', key, iv)").len(), 1);
    }

    #[test]
    fn flags_aes_256_ecb() {
        assert_eq!(run_on("algorithm: 'aes-256-ecb'").len(), 1);
    }

    #[test]
    fn allows_cbc_mode() {
        assert!(run_on("createCipheriv('aes-128-cbc', key, iv)").is_empty());
    }

    #[test]
    fn allows_gcm_mode() {
        assert!(run_on("createCipheriv('aes-256-gcm', key, iv)").is_empty());
    }
}
