//! OXC backend for react-no-usestate-high-frequency.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_use_state_setter_binding};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression,
};
use oxc_span::GetSpan;
use std::sync::Arc;

const HIGH_FREQ_EVENTS: &[&str] = &["mousemove", "scroll", "resize", "pointermove", "wheel"];
const HIGH_FREQ_JSX_PROPS: &[&str] = &[
    "onMouseMove",
    "onScroll",
    "onPointerMove",
    "onWheel",
    "onDrag",
    "onDragOver",
    "onTouchMove",
];

pub struct Check;

/// True when the handler spanning `handler_span` calls a React state setter —
/// an identifier that resolves to the setter slot of a `useState`/`useReducer`
/// destructure (`const [v, setV] = useState(...)`). Non-React `set`-prefixed
/// callees such as `setTimeout`/`setInterval`/`setHeaders` resolve to a
/// different binding shape (or none) and are not flagged.
fn handler_span_contains_setstate(
    handler_span: oxc_span::Span,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for n in semantic.nodes().iter() {
        let s = n.kind().span();
        if s.start < handler_span.start || s.end > handler_span.end {
            continue;
        }
        if let AstKind::CallExpression(call) = n.kind()
            && let Expression::Identifier(id) = &call.callee
            && is_use_state_setter_binding(id, semantic)
        {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                // Check for addEventListener("mousemove", handler)
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                if member.property.name.as_str() != "addEventListener" {
                    return;
                }
                if call.arguments.len() < 2 {
                    return;
                }
                // First arg must be a string literal with a high-freq event
                let Some(ev_lit) = call.arguments[0].as_expression().and_then(|e| {
                    if let Expression::StringLiteral(s) = e { Some(s) } else { None }
                }) else {
                    return;
                };
                let ev = ev_lit.value.as_str();
                if !HIGH_FREQ_EVENTS.contains(&ev) {
                    return;
                }
                // Second arg is the handler
                let Some(handler_expr) = call.arguments[1].as_expression() else {
                    return;
                };
                let handler_span = match handler_expr {
                    Expression::ArrowFunctionExpression(arrow) => arrow.span,
                    Expression::FunctionExpression(func) => func.span,
                    _ => return,
                };
                if !handler_span_contains_setstate(handler_span, semantic) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`setState` inside a high-frequency event listener (mousemove/scroll/...) — \
                             use `useRef` for the transient value and only commit a render when needed."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::JSXOpeningElement(opening) => {
                for attr_item in &opening.attributes {
                    let JSXAttributeItem::Attribute(attr) = attr_item else {
                        continue;
                    };
                    let JSXAttributeName::Identifier(name_ident) = &attr.name else {
                        continue;
                    };
                    let attr_name = name_ident.name.as_str();
                    if !HIGH_FREQ_JSX_PROPS.contains(&attr_name) {
                        continue;
                    }
                    let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                        continue;
                    };
                    let handler_span = match &ec.expression {
                        JSXExpression::ArrowFunctionExpression(arrow) => arrow.span,
                        JSXExpression::FunctionExpression(func) => func.span,
                        _ => continue,
                    };
                    if !handler_span_contains_setstate(handler_span, semantic) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`setState` inside a high-frequency JSX handler (onMouseMove/onScroll/...) — \
                                 use `useRef` for the transient value and only commit a render when needed."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
            _ => {}
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // Regression for #3239: `setTimeout` (and `clearTimeout`) inside a
    // `mousemove` handler is a browser timer API, not a React state setter, so
    // it must not flag.
    #[test]
    fn allows_settimeout_in_mousemove_listener() {
        let src = r#"
function setup_preload() {
    let mousemove_timeout;
    container.addEventListener('mousemove', (event) => {
        const target = event.target;
        clearTimeout(mousemove_timeout);
        mousemove_timeout = setTimeout(() => {
            void preload(target, PRELOAD_PRIORITIES.hover);
        }, 20);
    });
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_usestate_setter_in_mousemove_listener() {
        let src = r#"
function C() {
    const [x, setX] = useState(0);
    el.addEventListener("mousemove", (e) => { setX(e.clientX); });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_usestate_setter_in_onmousemove_jsx() {
        let src = r#"
function C() {
    const [x, setX] = useState(0);
    return <div onMouseMove={(e) => setX(e.clientX)} />;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #4015: a state-slot hole (`const [, setX] = useState(...)`)
    // still binds a render-scheduling setter, so a high-frequency handler calling
    // it must flag. `is_use_state_setter_binding` matches the setter slot only and
    // must not require slot 0 to be a plain identifier.
    #[test]
    fn flags_usestate_setter_with_state_slot_hole_in_mousemove() {
        let src = r#"
function C() {
    const [, setX] = useState(0);
    el.addEventListener("mousemove", (e) => { setX(e.clientX); });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_usereducer_dispatch_in_scroll_listener() {
        let src = r#"
function C() {
    const [state, dispatch] = useReducer(reducer, initial);
    el.addEventListener("scroll", () => { dispatch({ type: "tick" }); });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    // A `set`-prefixed user-land call that is not a `useState` setter (here a
    // local helper) writes to an external target, not React state.
    #[test]
    fn allows_non_state_set_helper_in_mousemove() {
        let src = r#"
function C() {
    const setHeaders = (h) => store.assign(h);
    el.addEventListener("mousemove", () => { setHeaders({ x: 1 }); });
}
"#;
        assert!(run(src).is_empty());
    }
}
