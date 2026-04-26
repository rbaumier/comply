//! ts-no-implicit-any-catch backend for TypeScript / TSX.
//!
//! Flags `catch (e) { ... }` — a catch binding with no type annotation. In
//! TypeScript that binding is implicitly typed as `any` (or `unknown` only
//! when `useUnknownInCatchVariables` is enabled), which defeats type
//! checking inside the handler. Users should write `catch (e: unknown)`
//! and narrow the value explicitly.
//!
//! Tree-sitter shape (tree-sitter-typescript grammar):
//!   catch_clause
//!     "catch" "("
//!     identifier                ← the binding name
//!     type_annotation?          ← optional sibling; present only when annotated
//!     ")"
//!     statement_block
//!
//! We flag a `catch_clause` whose direct children include an `identifier`
//! but no `type_annotation`. A `catch { ... }` with no binding has neither
//! child and is skipped (nothing to annotate).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["catch_clause"] => |node, source, ctx, diagnostics|
    let _ = source;
    let mut cursor = node.walk();
    let mut binding: Option<tree_sitter::Node> = None;
    let mut has_annotation = false;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "array_pattern" | "object_pattern" => {
                if binding.is_none() {
                    binding = Some(child);
                }
            }
            "type_annotation" => has_annotation = true,
            _ => {}
        }
    }
    let Some(binding) = binding else {
        // `catch { ... }` — no parameter, nothing to annotate.
        return;
    };
    if has_annotation {
        return;
    }
    let pos = binding.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-implicit-any-catch".into(),
        message: "catch binding has no type annotation — it defaults to `any`. \
                  Use `catch (e: unknown)` and narrow the value explicitly."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_catch_without_annotation() {
        let diags = run_on("try { f(); } catch (e) { log(e); }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "ts-no-implicit-any-catch");
    }

    #[test]
    fn allows_catch_with_unknown_annotation() {
        let diags = run_on("try { f(); } catch (e: unknown) { log(e); }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_catch_with_any_annotation() {
        let diags = run_on("try { f(); } catch (e: any) { log(e); }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_catch_without_binding() {
        let diags = run_on("try { f(); } catch { log('fail'); }");
        assert!(diags.is_empty());
    }
}
