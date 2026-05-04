use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useState"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let nodes = semantic.nodes();

        // Pass 1: collect (state_name, setter_name) pairs from
        // `const [state, setState] = useState(...)`.
        let mut pairs: Vec<(&str, &str)> = Vec::new();

        for node in nodes.iter() {
            let AstKind::VariableDeclarator(decl) = node.kind() else {
                continue;
            };
            let Some(init) = &decl.init else { continue };
            // init must be `useState(...)`
            let Expression::CallExpression(call) = init else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            if callee.name.as_str() != "useState" {
                continue;
            }
            // name must be array pattern [state, setter]
            let BindingPattern::ArrayPattern(arr) = &decl.id else {
                continue;
            };
            if arr.elements.len() != 2 {
                continue;
            }
            let (Some(first), Some(second)) = (&arr.elements[0], &arr.elements[1]) else {
                continue;
            };
            let BindingPattern::BindingIdentifier(state_id) = first else {
                continue;
            };
            let BindingPattern::BindingIdentifier(setter_id) = second else {
                continue;
            };
            let state = state_id.name.as_str();
            let setter = setter_id.name.as_str();
            if state.is_empty() || !setter.starts_with("set") {
                continue;
            }
            pairs.push((state, setter));
        }

        if pairs.is_empty() {
            return diagnostics;
        }

        // Pass 2: find `setter(state)` calls.
        for node in nodes.iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            let callee_name = callee.name.as_str();
            let Some(&(state, setter)) = pairs.iter().find(|(_, s)| *s == callee_name) else {
                continue;
            };
            if call.arguments.len() != 1 {
                continue;
            }
            let arg = &call.arguments[0];
            // Must be a plain identifier reference.
            let oxc_ast::ast::Argument::Identifier(arg_id) = arg else {
                continue;
            };
            if arg_id.name.as_str() != state {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{setter}({state})` is a no-op — setting state to its current value."
                ),
                severity: super::META.severity,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_setstate_with_own_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count);
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_pairs() {
        let src = r#"
const [name, setName] = useState("");
const [age, setAge] = useState(0);
setName(name);
setAge(age);
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_setter_with_different_value() {
        let src = r#"
const [count, setCount] = useState(0);
setCount(count + 1);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_with_new_value() {
        let src = r#"
const [name, setName] = useState("");
setName("hello");
"#;
        assert!(run_on(src).is_empty());
    }
}
