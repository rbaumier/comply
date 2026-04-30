//! Flag `import` statements that pull ORM entities into a NestJS controller
//! file. Detection is heuristic: the imported name ends with `Entity` or the
//! module path contains `entity`/`entities`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn is_nestjs_controller_file(source: &str) -> bool {
    source.contains("@Controller")
}

/// True if `line` looks like an entity import:
/// - `import { ... UserEntity ... } from '...'`
/// - `import x from '.../user.entity'`
/// - `import x from '.../entities/user'`
fn is_entity_import(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("import ") {
        return false;
    }
    // Module path heuristic.
    if trimmed.contains(".entity'")
        || trimmed.contains(".entity\"")
        || trimmed.contains("/entities/")
        || trimmed.contains("/entity/")
    {
        return true;
    }
    // Imported identifier ending in `Entity` inside `{ ... }`.
    let Some(open) = trimmed.find('{') else {
        return false;
    };
    let Some(close) = trimmed[open..].find('}') else {
        return false;
    };
    let names = &trimmed[open + 1..open + close];
    names
        .split(',')
        .map(|n| n.trim().trim_start_matches("type ").trim())
        .any(|name| {
            // Strip aliases: `Foo as Bar` → use the *imported* identifier `Foo`.
            let imported = name.split_whitespace().next().unwrap_or("");
            imported.ends_with("Entity") && imported != "Entity"
        })
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_nestjs_controller_file(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_entity_import(line) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Controller imports an ORM entity — return a DTO from the service \
                              instead of leaking the persistence model into the HTTP layer."
                        .to_string(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("ctrl.ts"), source))
    }

    #[test]
    fn flags_named_entity_import() {
        let src = "import { UserEntity } from './user.entity';\n@Controller() class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_entity_module_path() {
        let src = "import { User } from './entities/user';\n@Controller() class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_dto_import() {
        let src =
            "import { CreateUserDto } from './dto/create-user.dto';\n@Controller() class C {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_controller_files() {
        let src = "import { UserEntity } from './user.entity';\nclass Service {}";
        assert!(run(src).is_empty());
    }
}
