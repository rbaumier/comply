use crate::diagnostic::{Diagnostic, Severity};

const ATOMIC_TYPES: &[(&str, &str)] = &[
    ("bool", "AtomicBool"),
    ("usize", "AtomicUsize"),
    ("isize", "AtomicIsize"),
    ("u8", "AtomicU8"),
    ("u16", "AtomicU16"),
    ("u32", "AtomicU32"),
    ("u64", "AtomicU64"),
    ("i8", "AtomicI8"),
    ("i16", "AtomicI16"),
    ("i32", "AtomicI32"),
    ("i64", "AtomicI64"),
];

crate::ast_check! { on ["type_identifier"] prefilter = ["Mutex"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return };
    if text != "Mutex" { return; }

    let Some(parent) = node.parent() else { return };
    if parent.kind() != "generic_type" { return; }

    let Ok(full) = parent.utf8_text(source) else { return };

    for &(prim, atomic) in ATOMIC_TYPES {
        let pattern = format!("Mutex<{prim}>");
        if full == pattern {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &parent,
                super::META.id,
                format!("`{pattern}` — prefer `{atomic}` for lock-free access."),
                Severity::Warning,
            ));
            return;
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_mutex_bool() {
        let diags = run("static ERRORED: Mutex<bool> = Mutex::new(false);");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicBool"));
    }

    #[test]
    fn flags_mutex_usize() {
        let diags = run("static COUNT: Mutex<usize> = Mutex::new(0);");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicUsize"));
    }

    #[test]
    fn flags_mutex_u64() {
        let diags = run("static COUNTER: Mutex<u64> = Mutex::new(0);");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("AtomicU64"));
    }

    #[test]
    fn allows_mutex_string() {
        assert!(run("static DATA: Mutex<String> = Mutex::new(String::new());").is_empty());
    }

    #[test]
    fn allows_mutex_vec() {
        assert!(run("static DATA: Mutex<Vec<u8>> = Mutex::new(Vec::new());").is_empty());
    }

    #[test]
    fn allows_atomic_bool() {
        assert!(run("static ERRORED: AtomicBool = AtomicBool::new(false);").is_empty());
    }
}
