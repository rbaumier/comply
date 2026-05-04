//! ts-no-use-before-define oxc backend — accurate TDZ detection via
//! oxc_semantic scope/symbol analysis.

use oxc_semantic::SymbolFlags;
use oxc_span::GetSpan;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        for symbol_id in scoping.symbol_ids() {
            let flags = scoping.symbol_flags(symbol_id);
            if !flags.intersects(SymbolFlags::BlockScoped) {
                continue;
            }

            let decl_span = scoping.symbol_span(symbol_id);
            let name = scoping.symbol_name(symbol_id);

            for reference in scoping.get_resolved_references(symbol_id) {
                let ref_node_id = reference.node_id();
                let ref_span = nodes.kind(ref_node_id).span();
                if ref_span.start < decl_span.start {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, ref_span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("`{name}` is used before its definition."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_use_before_define() {
        let d = run_on("console.log(x); const x = 1;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_use_after_define() {
        assert!(run_on("const x = 1; console.log(x);").is_empty());
    }

    #[test]
    fn allows_function_declaration_hoisting() {
        assert!(run_on("f(); function f() {}").is_empty());
    }

    #[test]
    fn flags_class_used_before_define() {
        let d = run_on("const c = new C(); class C {}");
        assert_eq!(d.len(), 1, "classes are not hoisted, TDZ applies");
        assert!(d[0].message.contains("`C`"));
    }

    #[test]
    fn flags_use_before_define_from_nested_scope() {
        let d = run_on("const f = () => x; f(); let x = 1;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_var_hoisting() {
        assert!(run_on("console.log(x); var x = 1;").is_empty());
    }
}
