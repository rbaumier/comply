use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "memo" && func_text != "React.memo" { return; }

    // Extract the destructuring block `{ ... }` inside the component's parameter list.
    // Format: `memo(({ items = [] }) => ...)`. Scan with brace awareness so nested
    // `{}` in default values (e.g. `= () => {}`) don't close the outer destructure.
    let call_text = node.utf8_text(source).unwrap_or("");
    let brace_open = match call_text.find('{') {
        Some(i) => i,
        None => return,
    };
    let mut depth = 0i32;
    let mut brace_close: Option<usize> = None;
    for (i, ch) in call_text[brace_open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    brace_close = Some(brace_open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = match brace_close {
        Some(c) => c,
        None => return,
    };
    let params = &call_text[brace_open..=close];
    if params.contains("= []") || params.contains("= {}")
        || params.contains("= () =>") || params.contains("= new ")
    {
        let pos = func.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Non-primitive default prop inside `memo()` creates a new reference every render. Move it outside the component.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_array_default() {
        assert_eq!(run("const C = memo(({ items = [] }) => <div />)").len(), 1);
    }

    #[test]
    fn flags_object_default() {
        assert_eq!(run("const C = memo(({ style = {} }) => <div />)").len(), 1);
    }

    #[test]
    fn flags_fn_default() {
        assert_eq!(
            run("const C = memo(({ onClick = () => {} }) => <div />)").len(),
            1
        );
    }

    #[test]
    fn allows_primitive_default() {
        assert!(run("const C = memo(({ count = 0 }) => <span>{count}</span>)").is_empty());
    }

    #[test]
    fn allows_identifier_default() {
        assert!(
            run("const NOOP = () => {}; const C = memo(({ onClick = NOOP }) => <div />)")
                .is_empty()
        );
    }
}
