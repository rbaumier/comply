use crate::diagnostic::{Diagnostic, Severity};

fn imports_better_result(source: &str) -> bool {
    source.contains("better-result") || source.contains("@better-result")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !imports_better_result(ctx.source) {
        return;
    }
    if node.kind() != "catch_clause" {
        return;
    }
    let text = node.utf8_text(source).unwrap_or("");
    if text.contains("Panic") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Do not match/re-handle Panic in a catch — let it propagate.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_catch_panic() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { if (e instanceof Panic) {} }";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_catch_without_panic() {
        let src = "import { Result } from 'better-result';\ntry { f(); } catch (e) { log(e); }";
        assert!(run(src).is_empty());
    }
}
