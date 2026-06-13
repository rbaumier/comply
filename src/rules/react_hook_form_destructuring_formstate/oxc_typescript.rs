//! react-hook-form-destructuring-formstate oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["formState"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "formState" {
            return;
        }

        // Proxy tracking only matters for a direct read in the component/hook
        // body. An access nested inside a closure below that scope (e.g. an
        // `Object.defineProperties` getter or a callback) is deferred: it runs
        // when the closure is invoked, not at subscription time. Destructuring
        // it up front would snapshot eagerly and break reactivity, so skip it.
        if is_in_nested_function(node, semantic) {
            return;
        }

        let property = member.property.name.as_str();

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`formState.{property}` bypasses React Hook Form proxy tracking — destructure it: `const {{ {property} }} = formState;`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the access sits inside a closure nested below the function that
/// holds `formState` — i.e. there are two or more enclosing functions. The
/// innermost is the deferred closure; the outer one is the component/hook
/// scope. A direct read in the component/hook body has exactly one enclosing
/// function and is the genuine anti-pattern this rule targets.
fn is_in_nested_function<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut function_depth = 0u32;
    for kind in semantic.nodes().ancestor_kinds(node.id()) {
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            function_depth += 1;
            if function_depth >= 2 {
                return true;
            }
        }
    }
    false
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_direct_read_in_component_body() {
        let src = r#"
function Form() {
  const { formState } = useForm();
  if (formState.isValid) return null;
  return <div />;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn exempts_reads_inside_defineproperties_getters() {
        // Regression for rbaumier/comply#1869 — react-hook-form's own
        // useController.ts. Each access is inside a deferred `get` closure.
        let src = r#"
function useController(props) {
  const { formState } = useFormContext();
  const fieldState = React.useMemo(
    () =>
      Object.defineProperties(
        {},
        {
          invalid: { enumerable: true, get: () => !!get(formState.errors, name) },
          isDirty: { enumerable: true, get: () => !!get(formState.dirtyFields, name) },
          isTouched: { enumerable: true, get: () => !!get(formState.touchedFields, name) },
          isValidating: { enumerable: true, get: () => !!get(formState.validatingFields, name) },
          error: { enumerable: true, get: () => get(formState.errors, name) },
        },
      ),
    [formState, name],
  );
  return fieldState;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_read_inside_event_handler_closure() {
        let src = r#"
function Form() {
  const { formState } = useForm();
  const onClick = () => { if (formState.isValid) doThing(); };
  return <button onClick={onClick} />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_direct_read_in_custom_hook() {
        let src = r#"
function useThing() {
  const { formState } = useForm();
  return formState.isValid;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
