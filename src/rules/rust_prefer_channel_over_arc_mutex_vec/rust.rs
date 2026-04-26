//! rust-prefer-channel-over-arc-mutex-vec backend.
//!
//! Matches either `Arc::new(Mutex::new(Vec::new()))` construction chains
//! or `Arc<Mutex<Vec<_>>>` type annotations, gated on the file also
//! containing `.lock()` and `.push(` — the signal that the shared Vec is
//! being used as a collector across threads.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression", "generic_type"] => |node, source, ctx, diagnostics|
    if !ctx.source.contains(".lock()") || !ctx.source.contains(".push(") { return; }

    let matched = match node.kind() {
        "call_expression" => is_arc_mutex_vec_call(node, source),
        "generic_type" => is_arc_mutex_vec_type(node, source),
        _ => false,
    };
    if !matched { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use `mpsc::channel` instead of `Arc<Mutex<Vec>>` to collect results from concurrent tasks.".into(),
        Severity::Warning,
    ));
}

fn fn_text<'a>(call: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    call.child_by_field_name("function")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
}

fn first_arg(call: tree_sitter::Node) -> Option<tree_sitter::Node> {
    call.child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))
}

fn is_arc_path(text: &str) -> bool {
    text == "Arc::new" || text == "std::sync::Arc::new"
}

fn is_mutex_path(text: &str) -> bool {
    text == "Mutex::new" || text == "std::sync::Mutex::new" || text == "parking_lot::Mutex::new"
}

fn is_vec_ctor(call: tree_sitter::Node, source: &[u8]) -> bool {
    let t = fn_text(call, source);
    t == "Vec::new" || t == "std::vec::Vec::new" || t == "Vec::with_capacity"
}

fn is_arc_mutex_vec_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if !is_arc_path(fn_text(node, source)) { return false; }
    let Some(inner) = first_arg(node) else { return false; };

    let (mutex_call, target) = if inner.kind() == "call_expression" {
        (inner, first_arg(inner))
    } else {
        return false;
    };
    if !is_mutex_path(fn_text(mutex_call, source)) { return false; }
    let Some(target) = target else { return false; };

    match target.kind() {
        "call_expression" => is_vec_ctor(target, source),
        "macro_invocation" => target
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(source).ok())
            .is_some_and(|t| t == "vec"),
        _ => false,
    }
}

fn type_name<'a>(gt: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    gt.child_by_field_name("type")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
}

fn first_type_arg(gt: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let args = gt.child_by_field_name("type_arguments")?;
    let mut cursor = args.walk();
    args.named_children(&mut cursor).next()
}

fn is_arc_mutex_vec_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    let outer = type_name(node, source);
    if outer != "Arc" && outer != "std::sync::Arc" { return false; }
    let Some(mutex_ty) = first_type_arg(node) else { return false; };
    if mutex_ty.kind() != "generic_type" { return false; }
    let mutex_name = type_name(mutex_ty, source);
    if mutex_name != "Mutex" && mutex_name != "std::sync::Mutex" { return false; }
    let Some(vec_ty) = first_type_arg(mutex_ty) else { return false; };
    if vec_ty.kind() != "generic_type" { return false; }
    let vec_name = type_name(vec_ty, source);
    vec_name == "Vec" || vec_name == "std::vec::Vec"
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_arc_mutex_vec_with_push() {
        let src = "fn go() { let results = Arc::new(Mutex::new(Vec::new())); let r = results.clone(); thread::spawn(move || r.lock().unwrap().push(compute())); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_channel() {
        let src = "fn go() { let (tx, rx) = mpsc::channel(); thread::spawn(move || tx.send(compute()).unwrap()); let results: Vec<_> = rx.iter().collect(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arc_mutex_without_push() {
        assert!(run("fn go() { let x = Arc::new(Mutex::new(Vec::new())); }").is_empty());
    }
}
