use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.then(...)` or `.catch(...)`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop != "then" && prop != "catch" {
            return;
        }

        // Check if any argument callback contains `expect(...)`.
        if !args_contain_expect(&call.arguments, ctx.source) {
            return;
        }

        // Check if this call is returned or awaited.
        if is_returned_or_awaited(node, semantic) {
            return;
        }

        // Custom non-Promise thenable: a chain terminating in `.then(done)`
        // drives its own sequencing, so its assertions are not lost.
        if is_done_terminated_chain(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Promise `.{prop}()` with `expect()` inside must be returned or awaited."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if any descendant of the arguments contains a call to `expect(...)`.
fn args_contain_expect(args: &[Argument], source: &str) -> bool {
    for arg in args {
        use oxc_span::GetSpan;
        let span = arg.span();
        let text = &source[span.start as usize..span.end as usize];
        if text.contains("expect(") {
            return true;
        }
    }
    false
}

/// True if the `.then()`/`.catch()` chain that `node` belongs to is a custom
/// non-Promise thenable rather than a real Promise.
///
/// Genuine Promise chains are returned or awaited; a chain whose outermost
/// `.then()` instead passes the enclosing test's `done`-style callback (e.g.
/// `chainer.then(...).then(done)`) is a manual-sequencing builder that drives
/// its own execution, so its un-awaited assertions are not lost.
fn is_done_terminated_chain(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();

    // Climb to the outermost call of the member/call chain `node` sits in.
    let mut current_id = node.id();
    let mut outermost_call = node.id();
    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            break;
        }
        match parent.kind() {
            AstKind::StaticMemberExpression(_) | AstKind::ComputedMemberExpression(_) => {
                current_id = parent.id();
            }
            AstKind::CallExpression(_) => {
                outermost_call = parent.id();
                current_id = parent.id();
            }
            _ => break,
        }
    }

    let AstKind::CallExpression(call) = nodes.get_node(outermost_call).kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "then" {
        return false;
    }
    // The terminating `.then(done)` takes a single bare-identifier argument.
    let [Argument::Identifier(arg)] = call.arguments.as_slice() else {
        return false;
    };
    callback_param_in_scope(arg.name.as_str(), outermost_call, semantic)
}

/// True if `name` is a parameter of an enclosing function expression / arrow
/// (the test callback), i.e. the chain's terminator is the test's own
/// `done`-style callback rather than an external Promise consumer.
fn callback_param_in_scope(
    name: &str,
    from_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = from_id;
    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            return false;
        }
        let params = match parent.kind() {
            AstKind::Function(func) => &func.params,
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            _ => {
                current_id = parent.id();
                continue;
            }
        };
        for param in &params.items {
            if let BindingPattern::BindingIdentifier(ident) = &param.pattern
                && ident.name.as_str() == name
            {
                return true;
            }
        }
        current_id = parent.id();
    }
}

/// True if the call node is returned, awaited, or is an arrow function's expression body.
fn is_returned_or_awaited(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node.id();

    loop {
        let parent = nodes.parent_node(current_id);
        if parent.id() == current_id {
            return false;
        }
        match parent.kind() {
            AstKind::ReturnStatement(_) | AstKind::AwaitExpression(_) => return true,
            AstKind::ArrowFunctionExpression(arrow) => {
                // Only counts if this is the expression body (not inside a block body).
                return arrow.expression;
            }
            // Transparent wrappers — keep climbing.
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSTypeAssertion(_) => {
                current_id = parent.id();
            }
            _ => return false,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unhandled_then_with_expect() {
        let src = r#"
it('test', () => {
  promise.then(val => {
    expect(val).toBe(1);
  });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_returned_then() {
        let src = r#"
it('test', () => {
  return promise.then(val => {
    expect(val).toBe(1);
  });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_awaited_then() {
        let src = r#"
it('test', async () => {
  await promise.then(val => {
    expect(val).toBe(1);
  });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_catch_with_expect() {
        let src = r#"
it('test', () => {
  promise.catch(err => {
    expect(err).toBeDefined();
  });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    /// Regression for #1744: `.then()` on a custom non-Promise chainer whose
    /// chain terminates with the test's `done` callback must not be flagged.
    #[test]
    fn allows_done_terminated_custom_chainer() {
        let src = r#"
it('should render array', done => {
  waitForUpdate(() => {
    expect(vm.$el.innerHTML).toBe('<span>a</span>')
  })
    .then(() => {
      expect(vm.$el.innerHTML).toBe('<span>d</span>')
    })
    .then(done)
})
"#;
        assert!(run_on(src).is_empty());
    }

    /// An un-awaited `.then(callback)` whose chain does NOT terminate in a
    /// `done`-style identifier is still flagged.
    #[test]
    fn still_flags_then_not_terminated_by_done() {
        let src = r#"
it('test', done => {
  promise
    .then(() => {
      expect(x).toBe(1);
    })
    .then(() => {
      doSomething();
    });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
