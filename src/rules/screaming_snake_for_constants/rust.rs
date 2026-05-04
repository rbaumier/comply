use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["const_item", "static_item"] prefilter = ["const", "static"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if name == "_" { return; }

    if super::is_screaming_snake(name) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Constant `{name}` is not in `SCREAMING_SNAKE_CASE`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn allows_screaming_snake() {
        assert!(run("const MAX_RETRY: u32 = 3;").is_empty());
    }

    #[test]
    fn flags_camel_case() {
        let diags = run("const maxRetry: u32 = 3;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetry"));
    }

    #[test]
    fn allows_static_screaming() {
        assert!(run("static COUNTER: AtomicUsize = AtomicUsize::new(0);").is_empty());
    }

    #[test]
    fn flags_static_lowercase() {
        let diags = run("static counter: AtomicUsize = AtomicUsize::new(0);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_underscore() {
        assert!(run("const _: () = ();").is_empty());
    }
}
