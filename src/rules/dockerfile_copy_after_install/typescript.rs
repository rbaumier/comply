//! dockerfile-copy-after-install tree-sitter backend.
//!
//! `COPY . .` before `RUN npm ci` (or any package-manager install) busts the
//! layer cache on every source change. This rule walks instructions in order,
//! resetting per stage, and flags `COPY . <dest>` that appears before any
//! install RUN.

use crate::diagnostic::{Diagnostic, Severity};

const INSTALL_NEEDLES: &[&str] = &[
    "npm install",
    "npm ci",
    "yarn install",
    "pnpm install",
    "pip install",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }
    let mut install_seen = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "from_instruction" => {
                install_seen = false;
            }
            "run_instruction" => {
                if run_contains_install(child, source) {
                    install_seen = true;
                }
            }
            "copy_instruction" => {
                if !install_seen && copy_is_dot(child, source) {
                    let pos = child.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "`COPY . .` before dependency install — copy the lockfile, run install, then copy the rest to keep layer caching effective.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}

fn run_contains_install(run: tree_sitter::Node, source: &[u8]) -> bool {
    let text = run.utf8_text(source).unwrap_or("");
    INSTALL_NEEDLES.iter().any(|n| text.contains(n))
}

/// `COPY . <dest>` where the first non-flag path is exactly `.`.
fn copy_is_dot(copy: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = copy.walk();
    let paths: Vec<&str> = copy
        .children(&mut cursor)
        .filter(|c| c.kind() == "path")
        .filter_map(|c| c.utf8_text(source).ok())
        .collect();
    paths.len() >= 2 && paths[0] == "."
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_dockerfile(s, &Check)
    }

    #[test]
    fn flags_copy_before_install() {
        let src = "FROM node:22.12\nCOPY . .\nRUN npm ci\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_copy_after_install() {
        let src = "FROM node:22.12\nCOPY package.json package-lock.json ./\nRUN npm ci\nCOPY . .\n";
        assert!(run(src).is_empty());
    }
}
