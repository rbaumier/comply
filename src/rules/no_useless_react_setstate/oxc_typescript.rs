use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_semantic::SymbolId;
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

        // Pass 1: resolve each `const [state, setState] = useState(...)` to its
        // `(state_symbol, setter_symbol)` binding pair, so a later `setter(arg)`
        // is matched by symbol identity rather than by name.
        let mut pairs: Vec<(SymbolId, SymbolId)> = Vec::new();

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
            if state_id.name.as_str().is_empty() || !setter_id.name.as_str().starts_with("set") {
                continue;
            }
            let (Some(state_symbol), Some(setter_symbol)) =
                (state_id.symbol_id.get(), setter_id.symbol_id.get())
            else {
                continue;
            };
            pairs.push((state_symbol, setter_symbol));
        }

        if pairs.is_empty() {
            return diagnostics;
        }

        // Pass 2: flag `setter(arg)` only when both the callee and the argument
        // resolve to the state pair's bindings. A shadowing param, callback
        // param, or inner `const` of the same name resolves to a different
        // symbol and is left alone.
        let scoping = semantic.scoping();
        for node in nodes.iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::Identifier(callee) = &call.callee else {
                continue;
            };
            if call.arguments.len() != 1 {
                continue;
            }
            // Argument must be a plain identifier reference.
            let oxc_ast::ast::Argument::Identifier(arg_id) = &call.arguments[0] else {
                continue;
            };
            let Some(callee_symbol) = callee
                .reference_id
                .get()
                .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
            else {
                continue;
            };
            let Some(&(state_symbol, _)) = pairs.iter().find(|(_, s)| *s == callee_symbol) else {
                continue;
            };
            let Some(arg_symbol) = arg_id
                .reference_id
                .get()
                .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
            else {
                continue;
            };
            if arg_symbol != state_symbol {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{setter}({state})` is a no-op — setting state to its current value.",
                    setter = callee.name.as_str(),
                    state = arg_id.name.as_str(),
                ),
                severity: super::META.severity,
                span: None,
            });
        }

        diagnostics
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
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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

    #[test]
    fn allows_setter_with_shadowing_param() {
        let src = r#"
function ChangePasswordModal() {
  const [values, setValues] = useState<AnyMap>({});
  function handleValuesChange(_: any, values: any) {
    setValues(values);
  }
  return null;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_with_shadowing_inner_const() {
        let src = r#"
function Report() {
  const [responses, setResponses] = useState<any[]>([]);
  const load = () => {
    const responses = fields!.map(field => field);
    setResponses(responses);
  };
  return null;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_setter_with_shadowing_callback_param() {
        let src = r#"
function Select() {
  const [open, setOpen] = useState(false);
  function handleOpenChange(open: boolean) {
    setOpen(open);
  }
  return null;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_no_op_across_nested_scope() {
        let src = r#"
function Counter() {
  const [count, setCount] = useState(0);
  const reset = () => {
    setCount(count);
  };
  return null;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
