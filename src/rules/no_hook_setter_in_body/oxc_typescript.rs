//! OxcCheck backend for no-hook-setter-in-body — flag `useState` setter
//! called directly in a React component body (causes infinite re-renders).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Match `setFoo(...)` — identifier starting with "set" + at least one more char.
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        let name = id.name.as_str();
        if !name.starts_with("set") || name.len() <= 3 {
            return;
        }

        // Walk ancestors to determine context.
        let mut in_safe_scope = false;
        let mut in_component = false;
        let mut depth = 0u32;

        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::CallExpression(ancestor_call) => {
                    if let Expression::Identifier(callee_id) = &ancestor_call.callee {
                        let callee_name = callee_id.name.as_str();
                        if matches!(
                            callee_name,
                            "useEffect" | "useCallback" | "useMemo" | "useLayoutEffect"
                        ) {
                            in_safe_scope = true;
                            break;
                        }
                    }
                }
                AstKind::ObjectProperty(prop) => {
                    if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &prop.key {
                        let key_name = key.name.as_str();
                        if key_name.starts_with("on") || key_name.starts_with("handle") {
                            in_safe_scope = true;
                            break;
                        }
                    }
                }
                AstKind::VariableDeclarator(decl) => {
                    if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) =
                        &decl.id
                    {
                        let var_name = binding.name.as_str();
                        if var_name.starts_with("handle") || var_name.starts_with("on") {
                            in_safe_scope = true;
                            break;
                        }
                    }
                }
                AstKind::Function(func) => {
                    depth += 1;
                    if depth == 1 {
                        if let Some(ref id) = func.id {
                            let fn_name = id.name.as_str();
                            if fn_name
                                .starts_with(|c: char| c.is_ascii_uppercase())
                            {
                                in_component = true;
                            }
                        }
                    }
                }
                AstKind::ArrowFunctionExpression(_) => {
                    depth += 1;
                }
                _ => {}
            }
        }

        if !in_component || in_safe_scope {
            return;
        }

        if depth != 1 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}()` called directly in component body — causes infinite re-renders. Move to `useEffect` or an event handler."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_setter_in_body() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  setCount(1);
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_setter_in_use_effect() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  useEffect(() => {
    setCount(1);
  }, []);
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_in_event_handler() {
        let src = r#"
function App() {
  const [count, setCount] = useState(0);
  const handleClick = () => {
    setCount(count + 1);
  };
  return <div onClick={handleClick} />;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
