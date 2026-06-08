use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
        let bytes = name.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'I' || !bytes[1].is_ascii_uppercase() {
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
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_interface_prefix_i::oxc_typescript::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_i_prefix() {
        let diags = run("interface IUserRepository {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("UserRepository"));
    }


    #[test]
    fn flags_exported_i_prefix() {
        let diags = run("export interface IService {}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_normal_interface() {
        assert!(run("interface UserRepository {}").is_empty());
    }


    #[test]
    fn allows_lowercase_after_i() {
        assert!(run("interface Item {}").is_empty());
    }


    #[test]
    fn allows_single_letter() {
        assert!(run("interface I {}").is_empty());
    }


    #[test]
    fn flags_i_prefix_with_extends() {
        let diags = run("interface IProps extends BaseProps {}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn ignores_type_alias() {
        assert!(run("type IFoo = { x: number };").is_empty());
    }
}
