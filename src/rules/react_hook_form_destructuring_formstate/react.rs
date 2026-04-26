//! react-hook-form-destructuring-formstate AST backend.
//!
//! Flags `member_expression` nodes whose object is the identifier `formState`.
//! That pattern (e.g. `formState.isValid`, `formState.errors`) bypasses React
//! Hook Form's proxy-based field subscription — destructuring is required to
//! keep re-renders minimal.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    let Some(object) = node.child_by_field_name("object") else { return };
    if object.kind() != "identifier" {
        return;
    }
    let Ok(object_text) = object.utf8_text(source) else { return };
    if object_text != "formState" {
        return;
    }

    // Grab the property to include it in the message (best-effort).
    let property = node
        .child_by_field_name("property")
        .and_then(|p| p.utf8_text(source).ok())
        .unwrap_or("<field>");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`formState.{property}` bypasses React Hook Form proxy tracking — destructure it: `const {{ {property} }} = formState;`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_formstate_isvalid_access() {
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
    fn flags_formstate_errors_access() {
        let src = r#"
function Form() {
  const { formState } = useForm();
  return <div>{formState.errors.name?.message}</div>;
}
"#;
        // `formState.errors` is one match; the chained `.name` is on `errors`, not `formState`.
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_destructured_field() {
        let src = r#"
function Form() {
  const { formState: { isValid } } = useForm();
  if (isValid) return null;
  return <div />;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_formstate_member() {
        let src = r#"
function Form() {
  const state = getState();
  return <div>{state.isValid}</div>;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
