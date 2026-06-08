use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclarator(var_decl) = node.kind() else {
                continue;
            };

            let Some(init) = &var_decl.init else { continue };
            if !is_use_state_call(init) {
                continue;
            }

            let BindingPattern::ArrayPattern(arr) = &var_decl.id else {
                continue;
            };

            if arr.elements.len() < 2 {
                continue;
            }

            let Some(Some(value_pat)) = arr.elements.first() else {
                continue;
            };
            let Some(Some(setter_pat)) = arr.elements.get(1) else {
                continue;
            };

            let BindingPattern::BindingIdentifier(value_ident) = value_pat else {
                continue;
            };
            let BindingPattern::BindingIdentifier(setter_ident) = setter_pat else {
                continue;
            };

            let value_name = value_ident.name.as_str();
            let setter_name = setter_ident.name.as_str();

            if setter_name.starts_with('_') {
                continue;
            }

            let expected = expected_setter(value_name);
            if setter_name == expected {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, setter_ident.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "useState setter `{setter_name}` should be named `{expected}` \
                     to match the state variable `{value_name}`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn is_use_state_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(ident) => ident.name == "useState",
        Expression::StaticMemberExpression(member) => member.property.name == "useState",
        _ => false,
    }
}

fn expected_setter(value_name: &str) -> String {
    let mut s = String::from("set");
    let mut chars = value_name.chars();
    if let Some(c) = chars.next() {
        s.extend(c.to_uppercase());
        s.extend(chars);
    }
    s
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
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_correct_naming() {
        assert!(run_on("const [count, setCount] = useState(0);").is_empty());
    }

    #[test]
    fn flags_wrong_setter_name() {
        let d = run_on("const [count, updateCount] = useState(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setCount"));
    }

    #[test]
    fn allows_underscore_setter() {
        assert!(run_on("const [count, _setCount] = useState(0);").is_empty());
    }

    #[test]
    fn flags_react_dot_use_state() {
        let d = run_on("const [x, updateX] = React.useState(0);");
        assert_eq!(d.len(), 1);
    }
}
