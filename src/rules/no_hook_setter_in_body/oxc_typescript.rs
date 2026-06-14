//! OxcCheck backend for no-hook-setter-in-body — flag `useState` setter
//! called directly in a React component body (causes infinite re-renders).

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// The PascalCase-callee heuristic is too broad on its own: any PascalCase
/// function (e.g. a Vite plugin factory `TypeDocPlugin`) calling a `set*()`
/// method would match. A `set*()` call is only a React `useState` setter when
/// the file is actually React: a `.tsx`/`.jsx` file (JSX implies React) or a
/// `.ts`/`.js` module that imports React. Plain TypeScript is out of scope.
fn in_react_context(ctx: &CheckCtx) -> bool {
    matches!(ctx.lang, Language::Tsx) || crate::oxc_helpers::imports_react(ctx.source)
}

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
        if !in_react_context(ctx) {
            return;
        }

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
                    if depth == 1
                        && let Some(ref id) = func.id {
                            let fn_name = id.name.as_str();
                            if fn_name
                                .starts_with(|c: char| c.is_ascii_uppercase())
                            {
                                in_component = true;
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

    fn run_on_path(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
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

    #[test]
    fn allows_set_call_in_pascalcase_factory_in_plain_ts() {
        // Regression for #1739: a non-React PascalCase factory in a `.ts` file
        // with no React import is not a component; its `set*()` call is not a
        // hook setter.
        let src = r#"
export default function TypeDocPlugin(
  config: Partial<TypeDocOptions> = {}
): Plugin {
  const { serve, setTargetMode } = createTypeDocApp(config)
  setTargetMode('serve')

  return {
    name: 'typedoc',
    apply: 'serve',
    buildStart() {
      return serve()
    },
  }
}
"#;
        assert!(run_on_path(src, "vite-typedoc-plugin.ts").is_empty());
    }

    #[test]
    fn flags_setter_in_plain_ts_that_imports_react() {
        let src = r#"
import { useState } from 'react';
function App() {
  const [count, setCount] = useState(0);
  setCount(1);
  return null;
}
"#;
        assert_eq!(run_on_path(src, "app.ts").len(), 1);
    }
}
