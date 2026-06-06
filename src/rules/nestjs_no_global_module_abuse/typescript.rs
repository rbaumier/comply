//! Flag any `@Global()` decorator in NestJS files.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/")
}

crate::ast_check! { on ["decorator"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    if !text.starts_with("@Global") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`@Global()` modules hide dependencies — import the module explicitly where needed.".into(),
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
    fn flags_global_module() {
        let src = "import { Global, Module } from '@nestjs/common';\n@Global() @Module({}) export class CommonModule {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_global_module() {
        let src = "import { Module } from '@nestjs/common';\n@Module({}) export class CommonModule {}";
        assert!(run(src).is_empty());
    }
}
