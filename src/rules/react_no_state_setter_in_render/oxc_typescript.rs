//! react-no-state-setter-in-render OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_span::GetSpan;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn starts_with_use_hook(name: &str) -> bool {
    name.starts_with("use") && name.chars().nth(3).is_some_and(|c| c.is_ascii_uppercase())
}

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            // Look for function declarations and arrow functions assigned to variables.
            let func_name = match node.kind() {
                AstKind::Function(func) => {
                    let Some(id) = &func.id else { continue };
                    if func.body.is_none() {
                        continue;
                    }
                    id.name.as_str().to_string()
                }
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::BindingIdentifier(id) = &decl.id else {
                        continue;
                    };
                    let Some(init) = &decl.init else { continue };
                    match init {
                        Expression::ArrowFunctionExpression(arrow) => {
                            if arrow.body.statements.first().is_none() {
                                continue;
                            }
                            id.name.as_str().to_string()
                        }
                        Expression::FunctionExpression(func) => {
                            if func.body.is_none() {
                                continue;
                            }
                            id.name.as_str().to_string()
                        }
                        _ => continue,
                    }
                }
                _ => continue,
            };

            if !starts_with_uppercase(&func_name) && !starts_with_use_hook(&func_name) {
                continue;
            }

            // Map each useState setter to its paired state variable in this function.
            let setters = collect_setters_oxc(node, semantic, ctx);
            if setters.is_empty() {
                continue;
            }

            // Walk the function's direct body for setter calls — skip nested functions.
            find_setter_calls_oxc(node, semantic, ctx, &setters, &func_name, &mut diagnostics);
        }

        diagnostics
    }
}

/// Map each setter to its paired state variable from `const [x, setX] = useState(...)`
/// patterns: `setX` → `x`. The state name (`None` when slot 0 is not a plain binding
/// identifier, e.g. `const [, setX] = ...`) lets the caller exempt the React-sanctioned
/// "adjust state during render" guard precisely — only a guard referencing the *paired*
/// state variable terminates.
fn collect_setters_oxc(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    ctx: &CheckCtx,
) -> FxHashMap<String, Option<String>> {
    let mut setters = FxHashMap::default();
    let nodes = semantic.nodes();

    for node in nodes.iter() {
        // Must be a descendant of the function node.
        if !is_descendant_of(node.id(), func_node.id(), nodes) {
            continue;
        }

        let AstKind::VariableDeclarator(decl) = node.kind() else {
            continue;
        };
        let Some(init) = &decl.init else { continue };
        let Expression::CallExpression(call) = init else {
            continue;
        };
        let callee_text = &ctx.source
            [call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "useState" && !callee_text.ends_with(".useState") {
            continue;
        }
        let BindingPattern::ArrayPattern(arr) = &decl.id else {
            continue;
        };
        // Second slot is the setter; first slot is the paired state variable.
        if let Some(Some(BindingPattern::BindingIdentifier(setter_id))) = arr.elements.get(1) {
            let state_name = match arr.elements.first() {
                Some(Some(BindingPattern::BindingIdentifier(state_id))) => {
                    Some(state_id.name.as_str().to_string())
                }
                _ => None,
            };
            setters.insert(setter_id.name.as_str().to_string(), state_name);
        }
    }

    setters
}

/// Find direct calls to setter names in the function body, skipping nested functions.
fn find_setter_calls_oxc(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    ctx: &CheckCtx,
    setters: &FxHashMap<String, Option<String>>,
    _func_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let nodes = semantic.nodes();

    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = &call.callee else {
            continue;
        };
        let Some(paired_state) = setters.get(callee.name.as_str()) else {
            continue;
        };

        // Must be a descendant of the function node.
        if !is_descendant_of(node.id(), func_node.id(), nodes) {
            continue;
        }

        // Must NOT be inside a nested function (arrow, function expression, etc.).
        if is_inside_nested_function(node.id(), func_node.id(), nodes) {
            continue;
        }

        // React-sanctioned "adjust state during render": a setter guarded by an
        // `if`/ternary whose test references the *paired* state variable terminates
        // (once state matches, the guard is false and React bails out). Exempt it.
        if let Some(state) = paired_state {
            let mut state_names = FxHashSet::default();
            state_names.insert(state.clone());
            if crate::oxc_helpers::is_guarded_derive_during_render(
                node.id(),
                &state_names,
                func_node.id(),
                semantic,
            ) {
                continue;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}(...)` is called directly during render — this triggers an infinite \
                 render loop. Move the call into a handler, `useEffect`, or compute the value \
                 inline instead of storing it.",
                callee.name.as_str()
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn is_descendant_of(
    node_id: oxc_semantic::NodeId,
    ancestor_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    if node_id == ancestor_id {
        return true;
    }
    let mut cur = node_id;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return false;
        }
        if parent == ancestor_id {
            return true;
        }
        cur = parent;
    }
}

fn is_inside_nested_function(
    node_id: oxc_semantic::NodeId,
    func_node_id: oxc_semantic::NodeId,
    nodes: &oxc_semantic::AstNodes,
) -> bool {
    let mut cur = node_id;
    loop {
        let parent_id = nodes.parent_id(cur);
        if parent_id == cur {
            return false;
        }
        if parent_id == func_node_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                return true;
            }
            _ => {}
        }
        cur = parent_id;
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

    #[test]
    fn flags_unconditional_setter_in_render() {
        let src = "function Counter() { const [n, setN] = useState(0); setN(1); return null; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_setter_in_event_handler() {
        let src = "function Counter() { const [n, setN] = useState(0); return <button onClick={() => setN(1)} />; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_setter_in_useeffect() {
        let src = "function Counter() { const [n, setN] = useState(0); useEffect(() => { setN(1); }, []); return null; }";
        assert!(run(src).is_empty());
    }

    // --- #3984: React-sanctioned "adjust state during render" is exempt ---

    #[test]
    fn allows_guarded_setter_color_handle() {
        // ColorHandle.tsx repro: guard compares incoming prop against paired state.
        let src = r#"
function ColorHandle({isOpen}) {
  let [state, setState] = useState(isOpen ? 'open' : 'closed');
  if (isOpen && state === 'closed') {
    setState('open');
  }
  if (!isOpen && state === 'open') {
    setState('exiting');
  }
  return null;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_guarded_setter_dialog_container() {
        // DialogContainer.tsx repro: guard on `child !== lastChild`.
        let src = r#"
function DialogContainer({child}) {
  let [lastChild, setLastChild] = useState(null);
  if (child && child !== lastChild) {
    setLastChild(child);
  }
  return null;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_guarded_setter_action_bar() {
        // ActionBar.tsx repro: guard on `selectedItemCount !== lastCount`.
        let src = r#"
function ActionBar({selectedItemCount}) {
  let [lastCount, setLastCount] = useState(selectedItemCount);
  if ((selectedItemCount === 'all' || selectedItemCount > 0) && selectedItemCount !== lastCount) {
    setLastCount(selectedItemCount);
  }
  return null;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_guarded_setter_in_ternary_branch() {
        let src = r#"
function Comp({isOpen}) {
  let [state, setState] = useState('closed');
  state === 'closed' ? setState('open') : null;
  return null;
}
"#;
        assert!(run(src).is_empty());
    }

    // --- false-negative guards: the exemption must stay narrow ---

    #[test]
    fn flags_setter_guarded_by_unrelated_condition() {
        // Guard does NOT reference the paired state var (`state`), so it does not
        // terminate — still a potential infinite loop, still flagged.
        let src = r#"
function Comp({someProp}) {
  let [state, setState] = useState('closed');
  if (someProp) {
    setState('open');
  }
  return null;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_setter_guarded_by_other_states_state() {
        // Guard references a DIFFERENT state variable (`other`), not the one paired
        // with `setState`, so it can loop and stays flagged.
        let src = r#"
function Comp({prop}) {
  let [state, setState] = useState('closed');
  let [other, setOther] = useState(0);
  if (other > 0) {
    setState('open');
  }
  return null;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_guarded_setter_with_no_state_slot() {
        // The destructure omits the state slot (`const [, setX]`), so there is no
        // paired state variable to compare against — the guard cannot be proven to
        // terminate and the call stays flagged.
        let src = r#"
function Comp({flag}) {
  let [, setState] = useState('closed');
  if (flag) {
    setState('open');
  }
  return null;
}
"#;
        assert_eq!(run(src).len(), 1);
    }
}

