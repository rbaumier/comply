use crate::diagnostic::{Diagnostic, Severity};

const GENERIC_CONSTRUCTORS: &[&str] = &["Map", "Set", "WeakMap", "WeakSet", "Array", "Promise"];

const GENERIC_FUNCTIONS: &[&str] = &[
    "useState", "useRef", "useCallback", "useMemo", "useContext", "useReducer",
    "useImperativeHandle", "createContext", "forwardRef", "memo", "lazy",
    "createStore", "defineStore", "ref", "reactive", "computed",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "new_expression" => check_new_expression(node, source, ctx, diagnostics),
        "call_expression" => check_call_expression(node, source, ctx, diagnostics),
        _ => {}
    }
}

fn check_new_expression(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(constructor) = node.child_by_field_name("constructor") else { return; };
    if node.child_by_field_name("type_arguments").is_some() { return; }
    let name = constructor.utf8_text(source).unwrap_or("");
    if !GENERIC_CONSTRUCTORS.contains(&name) { return; }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "require-type-arguments".into(),
        message: format!("`new {name}()` requires explicit type arguments — add `<K, V>` or similar."),
        severity: Severity::Warning,
        span: None,
    });
}

fn check_call_expression(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(func) = node.child_by_field_name("function") else { return; };
    if node.child_by_field_name("type_arguments").is_some() { return; }
    let name = match func.kind() {
        "identifier" => func.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            if let Some(prop) = func.child_by_field_name("property") {
                prop.utf8_text(source).unwrap_or("")
            } else { return; }
        }
        _ => return,
    };
    if !GENERIC_FUNCTIONS.contains(&name) { return; }
    if name == "useState" || name == "useRef" {
        if let Some(args) = node.child_by_field_name("arguments") {
            if args.named_child_count() > 0 { return; }
        }
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "require-type-arguments".into(),
        message: format!("`{name}()` requires explicit type arguments — add `<T>` or similar."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run_on(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }
    fn run_tsx(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_tsx(code, &Check) }

    #[test]
    fn flags_new_map() { assert_eq!(run_on("const m = new Map();").len(), 1); }
    #[test]
    fn allows_new_map_with_types() { assert!(run_on("const m = new Map<string, number>();").is_empty()); }
    #[test]
    fn flags_use_state_no_initial() { assert_eq!(run_tsx("const [x, setX] = useState();").len(), 1); }
    #[test]
    fn allows_use_state_with_initial() { assert!(run_tsx("const [x, setX] = useState(0);").is_empty()); }
    #[test]
    fn allows_use_state_with_types() { assert!(run_tsx("const [x, setX] = useState<number>();").is_empty()); }
    #[test]
    fn flags_create_context() { assert_eq!(run_tsx("const Ctx = createContext();").len(), 1); }
    #[test]
    fn allows_regular_call() { assert!(run_on("const x = someFunction();").is_empty()); }
}
