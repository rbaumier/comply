use crate::diagnostic::{Diagnostic, Severity};

const EXPENSIVE_PREFIXES: &[&str] = &[
    "localStorage.",
    "sessionStorage.",
    "JSON.parse(",
    "compute",
    "build",
    "create",
    "parse(",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.utf8_text(source).unwrap_or("") != "useState" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let init = match args.named_child(0) {
        Some(i) => i,
        None => return,
    };
    match init.kind() {
        "number" | "string" | "true" | "false" | "null" | "undefined"
        | "arrow_function" | "identifier" => return,
        "call_expression" => {}
        _ => return,
    }
    let init_text = init.utf8_text(source).unwrap_or("");
    if EXPENSIVE_PREFIXES.iter().any(|p| init_text.starts_with(p)) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Pass a lazy initializer `() => expr` to `useState` to avoid recomputing on every render.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_local_storage() {
        assert_eq!(run("useState(localStorage.getItem('x'))").len(), 1);
    }

    #[test]
    fn flags_json_parse() {
        assert_eq!(run("useState(JSON.parse(raw))").len(), 1);
    }

    #[test]
    fn allows_lazy_init() {
        assert!(run("useState(() => localStorage.getItem('x'))").is_empty());
    }

    #[test]
    fn allows_primitive() {
        assert!(run("useState(0)").is_empty());
        assert!(run("useState(false)").is_empty());
        assert!(run("useState(null)").is_empty());
    }

    #[test]
    fn allows_identifier() {
        assert!(run("useState(initialValue)").is_empty());
    }
}
