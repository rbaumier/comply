use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{source_type_for_path, with_semantic};
use crate::rules::backend::CheckCtx;

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_type = source_type_for_path(ctx.path);
        with_semantic(ctx.source, source_type, |semantic| {
            let mut diagnostics = Vec::new();

            for node in semantic.nodes() {
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

                let span = setter_ident.span();
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line,
                    column,
                    rule_id: "hook-use-state".into(),
                    message: format!(
                        "useState setter `{setter_name}` should be named `{expected}` \
                         to match the state variable `{value_name}`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            diagnostics
        })
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

fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn allows_correct_naming() {
        assert!(run_on("const [count, setCount] = useState(0);").is_empty());
    }

    #[test]
    fn allows_is_prefix() {
        assert!(run_on("const [isOpen, setIsOpen] = useState(false);").is_empty());
    }

    #[test]
    fn flags_wrong_setter_name() {
        let d = run_on("const [count, updateCount] = useState(0);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setCount"));
    }

    #[test]
    fn allows_single_element() {
        assert!(run_on("const [count] = useState(0);").is_empty());
    }

    #[test]
    fn allows_non_use_state() {
        assert!(run_on("const [data, setData] = useQuery();").is_empty());
    }

    #[test]
    fn allows_underscore_setter() {
        assert!(run_on("const [count, _setCount] = useState(0);").is_empty());
    }

    #[test]
    fn allows_no_destructuring() {
        assert!(run_on("const state = useState(0);").is_empty());
    }

    #[test]
    fn flags_react_dot_use_state() {
        let d = run_on("const [x, updateX] = React.useState(0);");
        assert_eq!(d.len(), 1);
    }
}
