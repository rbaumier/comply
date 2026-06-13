use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Initialisms that begin with `I` and are commonly followed by a lowercased
/// version or segment (e.g. `IPv4`, `IPv6`). Listed longest-first so the longest
/// matching initialism wins (`IPv4` before `IP`). A name is exempt only when it
/// starts with one of these AS A COMPLETE TOKEN — the next character is
/// uppercase, a digit, or end-of-string — so `IOrder` (lowercase `r` after `IO`)
/// stays flagged as a Hungarian `I` on "Order".
const I_INITIALISMS: &[&str] = &["IPv4", "IPv6", "IPC", "IP", "IO"];

/// Returns true when `name` carries a Hungarian-notation `I` prefix that should
/// be flagged.
///
/// Combines two signals:
/// 1. Shape: a Hungarian prefix is `I` followed by an uppercase letter THEN a
///    lowercase letter (`I[A-Z][a-z]`). When `I` is followed by two uppercase
///    letters the leading `I` is part of an initialism (`IPRule`, `IORule`) and
///    is not a prefix.
/// 2. Curated initialisms: names starting with an entry of `I_INITIALISMS` as a
///    complete token (next char uppercase/digit/end) are initialisms, not
///    prefixes, even when the shape rule would treat them as one (`IPv4Foo`).
fn is_hungarian_i_prefix(name: &str) -> bool {
    let bytes = name.as_bytes();
    // Need at least `I` + an uppercase letter to even consider a prefix.
    if bytes.len() < 2 || bytes[0] != b'I' || !bytes[1].is_ascii_uppercase() {
        return false;
    }
    // Shape rule: two consecutive uppercase after `I` means the `I` belongs to
    // an initialism, not a Hungarian prefix.
    if bytes[2..].first().is_some_and(u8::is_ascii_uppercase) {
        return false;
    }
    // Curated initialisms: exempt when `name` begins with one as a complete
    // token (next char uppercase, digit, or end-of-string).
    for initialism in I_INITIALISMS {
        if let Some(rest) = name.strip_prefix(initialism) {
            let token_boundary = rest
                .as_bytes()
                .first()
                .is_none_or(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
            if token_boundary {
                return false;
            }
        }
    }
    true
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSInterfaceDeclaration(iface) = node.kind() else {
            return;
        };

        let name = iface.id.name.as_str();
        if !is_hungarian_i_prefix(name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, iface.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Interface `{name}` uses the `I` prefix — rename to `{}`.",
                &name[1..]
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
