//! number-literal-case — enforce lowercase prefix/exponent, uppercase hex digits.

use crate::diagnostic::{Diagnostic, Severity};

/// The canonical form: lowercase prefix/exponent, uppercase hex digits.
fn canonical(raw: &str) -> Option<String> {
    let (body, suffix) = if let Some(stripped) = raw.strip_suffix('n') {
        (stripped, "n")
    } else {
        (raw, "")
    };

    if body.len() < 2 {
        return None;
    }

    let prefix_lower = body[..2].to_lowercase();
    let fixed = match prefix_lower.as_str() {
        "0x" => {
            let digits = &body[2..];
            format!("0x{}{}", digits.to_uppercase(), suffix)
        }
        "0b" | "0o" => {
            format!("{}{}{}", prefix_lower, &body[2..], suffix)
        }
        _ => {
            if !body.contains('E') && !body.contains('e') {
                return None;
            }
            let lowered = body.to_lowercase();
            format!("{}{}", lowered, suffix)
        }
    };

    if fixed == raw { None } else { Some(fixed) }
}

crate::ast_check! { on ["number"] => |node, source, ctx, diagnostics|
    let raw = node.utf8_text(source).unwrap_or("");
    if let Some(fixed) = canonical(raw) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "number-literal-case".into(),
            message: format!(
                "Invalid number literal casing: `{}` should be `{}`.",
                raw, fixed
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn flags_uppercase_hex_prefix() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const x = 0XFF;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_lowercase_hex_digits() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const x = 0xff;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFF"));
    }

    #[test]
    fn flags_uppercase_exponent() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const x = 1E3;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("1e3"));
    }

    #[test]
    fn flags_uppercase_binary_prefix() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const x = 0B1010;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0b1010"));
    }

    #[test]
    fn flags_uppercase_octal_prefix() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const x = 0O777;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0o777"));
    }

    #[test]
    fn allows_correct_hex() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = 0xFF;", "t.ts").is_empty());
    }

    #[test]
    fn allows_correct_exponent() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = 1e3;", "t.ts").is_empty());
    }

    #[test]
    fn allows_correct_binary() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "const x = 0b1010;", "t.ts").is_empty());
    }

    #[test]
    fn flags_bigint_hex() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const x = 0XFFn;", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("0xFFn"));
    }
}
