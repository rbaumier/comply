//! Flag `forwardRef(() => Foo)` calls in NestJS files.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    source.contains("@nestjs/")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "identifier" { return; }
    let text = std::str::from_utf8(&source[callee.byte_range()]).unwrap_or("");
    if text != "forwardRef" { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`forwardRef()` indicates a circular dependency — refactor to break the cycle.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_forward_ref() {
        let src = "import { forwardRef } from '@nestjs/common';\nconst x = forwardRef(() => Foo);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_non_nestjs() {
        let src = "function forwardRef(f: any) { return f(); }\nconst x = forwardRef(() => 1);";
        assert!(run(src).is_empty());
    }
}
