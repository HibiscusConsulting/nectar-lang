use std::path::{Path, PathBuf};
use std::collections::HashMap;

use crate::ast::*;
use crate::lexer::Lexer;
use crate::module_resolver::ModuleResolver;
use crate::parser::Parser;
#[cfg(test)]
use crate::token::Span;

/// Multi-file compilation orchestrator.
///
/// Recursively resolves `mod` declarations, loads and parses each source file,
/// and merges all module ASTs into a single `Program` with namespaced items.
pub struct ModuleLoader {
    resolver: ModuleResolver,
    /// Parsed modules keyed by their canonical file path.
    modules: HashMap<PathBuf, Vec<Item>>,
}

#[derive(Debug)]
pub struct ModuleLoadError {
    pub message: String,
}

impl std::fmt::Display for ModuleLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ModuleLoadError {}

impl ModuleLoader {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            resolver: ModuleResolver::new(root_dir),
            modules: HashMap::new(),
        }
    }

    /// Entry point: compile a multi-file project starting from the given file.
    ///
    /// Returns a merged `Program` containing all items from all modules,
    /// with `mod` declarations resolved and external files loaded.
    pub fn compile_project(entry_path: &Path) -> Result<Program, ModuleLoadError> {
        let root_dir = entry_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let mut loader = Self::new(root_dir);

        let items = loader.load_and_parse(entry_path)?;
        let resolved = loader.resolve_items(items, entry_path)?;

        Ok(Program { items: resolved })
    }

    /// Load a file, lex it, and parse it into items.
    fn load_and_parse(&mut self, path: &Path) -> Result<Vec<Item>, ModuleLoadError> {
        // Check for circular dependencies
        if !self.resolver.mark_loaded(path) {
            return Err(ModuleLoadError {
                message: format!("circular module dependency detected: {}", path.display()),
            });
        }

        let source = self.resolver.load_module(path).map_err(|e| ModuleLoadError {
            message: e.message,
        })?;

        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().map_err(|e| ModuleLoadError {
            message: format!("lex error in {}: {}", path.display(), e),
        })?;

        let mut parser = Parser::new(tokens);
        let (program, errors) = parser.parse_program_recovering();

        if !errors.is_empty() {
            let msgs: Vec<String> = errors
                .iter()
                .map(|e| format!("  {}:{}: {}", e.span.line, e.span.col, e.message))
                .collect();
            return Err(ModuleLoadError {
                message: format!(
                    "parse errors in {}:\n{}",
                    path.display(),
                    msgs.join("\n")
                ),
            });
        }

        Ok(program.items)
    }

    /// Walk all items, resolve `mod` declarations (loading external files),
    /// and namespace module items.
    fn resolve_items(
        &mut self,
        items: Vec<Item>,
        current_file: &Path,
    ) -> Result<Vec<Item>, ModuleLoadError> {
        let current_dir = current_file
            .parent()
            .unwrap_or_else(|| Path::new("."));

        let mut resolved = Vec::new();

        for item in items {
            match item {
                Item::Mod(mod_def) => {
                    let mod_items = if mod_def.is_external {
                        // `mod foo;` — load from file
                        let mod_path = self
                            .resolver
                            .resolve_module_from(current_dir, &[mod_def.name.clone()])
                            .map_err(|e| ModuleLoadError { message: e.message })?;

                        let items = self.load_and_parse(&mod_path)?;
                        self.resolve_items(items, &mod_path)?
                    } else if let Some(items) = mod_def.items {
                        // `mod foo { ... }` — inline module
                        self.resolve_items(items, current_file)?
                    } else {
                        vec![]
                    };

                    // Wrap the module's items in an inline mod
                    resolved.push(Item::Mod(ModDef {
                        name: mod_def.name,
                        items: Some(mod_items),
                        is_external: false,
                        span: mod_def.span,
                    }));
                }
                other => resolved.push(other),
            }
        }

        Ok(resolved)
    }
}

/// Convenience function: check whether a program has any `mod` declarations
/// that need multi-file resolution.
pub fn has_mod_declarations(program: &Program) -> bool {
    program.items.iter().any(|item| matches!(item, Item::Mod(_)))
}

/// Collect all imported names from `use` statements in a program.
/// Returns a map from local name -> (module path segments, original name).
pub fn collect_imports(program: &Program) -> HashMap<String, (Vec<String>, String)> {
    let mut imports = HashMap::new();

    for item in &program.items {
        if let Item::Use(use_path) = item {
            if use_path.glob {
                // Glob imports can't be resolved without the module contents
                continue;
            }

            if let Some(group) = &use_path.group {
                // Group imports: `use foo::bar::{A, B as C};`
                for group_item in group {
                    let local_name = group_item
                        .alias
                        .as_ref()
                        .unwrap_or(&group_item.name)
                        .clone();
                    imports.insert(
                        local_name,
                        (use_path.segments.clone(), group_item.name.clone()),
                    );
                }
            } else if use_path.segments.len() >= 2 {
                // Single import: `use foo::Bar;` or `use foo::Bar as Baz;`
                let original_name = use_path.segments.last().unwrap().clone();
                let local_name = use_path
                    .alias
                    .as_ref()
                    .unwrap_or(&original_name)
                    .clone();
                let module_path = use_path.segments[..use_path.segments.len() - 1].to_vec();
                imports.insert(local_name, (module_path, original_name));
            }
        }
    }

    imports
}

/// Find all public items in a list of items.
pub fn public_items(items: &[Item]) -> Vec<&str> {
    let mut names = Vec::new();
    for item in items {
        match item {
            Item::Function(f) if f.is_pub => names.push(f.name.as_str()),
            Item::Struct(s) if s.is_pub => names.push(s.name.as_str()),
            Item::Enum(e) if e.is_pub => names.push(e.name.as_str()),
            Item::Store(s) if s.is_pub => names.push(s.name.as_str()),
            _ => {}
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_compile_single_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let main_file = dir.path().join("main.nectar");
        fs::write(&main_file, "fn main() -> i32 { 42 }").unwrap();

        let program = ModuleLoader::compile_project(&main_file).unwrap();
        assert_eq!(program.items.len(), 1);
    }

    #[test]
    fn test_compile_with_inline_mod() {
        let dir = tempfile::TempDir::new().unwrap();
        let main_file = dir.path().join("main.nectar");
        fs::write(
            &main_file,
            "mod utils {\n  pub fn helper() -> i32 { 1 }\n}\nfn main() -> i32 { 0 }",
        )
        .unwrap();

        let program = ModuleLoader::compile_project(&main_file).unwrap();
        // Should have: mod utils + fn main
        assert_eq!(program.items.len(), 2);
    }

    #[test]
    fn test_compile_with_external_mod() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create utils.nectar
        fs::write(
            dir.path().join("utils.nectar"),
            "pub fn helper() -> i32 { 1 }",
        )
        .unwrap();

        // Create main.nectar that references it
        let main_file = dir.path().join("main.nectar");
        fs::write(&main_file, "mod utils;\nfn main() -> i32 { 0 }").unwrap();

        let program = ModuleLoader::compile_project(&main_file).unwrap();
        assert_eq!(program.items.len(), 2);

        // The mod should now be inline with its items
        if let Item::Mod(m) = &program.items[0] {
            assert_eq!(m.name, "utils");
            assert!(m.items.is_some());
            assert_eq!(m.items.as_ref().unwrap().len(), 1);
        } else {
            panic!("expected Mod item");
        }
    }

    #[test]
    fn test_compile_with_mod_dir() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create mymod/mod.nectar
        fs::create_dir_all(dir.path().join("mymod")).unwrap();
        fs::write(
            dir.path().join("mymod").join("mod.nectar"),
            "pub fn greet() -> i32 { 42 }",
        )
        .unwrap();

        let main_file = dir.path().join("main.nectar");
        fs::write(&main_file, "mod mymod;\nfn main() -> i32 { 0 }").unwrap();

        let program = ModuleLoader::compile_project(&main_file).unwrap();
        assert_eq!(program.items.len(), 2);
    }

    #[test]
    fn test_circular_dependency_detected() {
        let dir = tempfile::TempDir::new().unwrap();

        // a.nectar loads b, b.nectar loads a
        fs::write(dir.path().join("a.nectar"), "mod b;").unwrap();
        fs::write(dir.path().join("b.nectar"), "mod a;").unwrap();

        let main_file = dir.path().join("a.nectar");
        let result = ModuleLoader::compile_project(&main_file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("circular"),
            "expected circular dependency error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_collect_imports_single() {
        let program = Program {
            items: vec![Item::Use(UsePath {
                segments: vec!["math".into(), "Vec3".into()],
                alias: None,
                glob: false,
                group: None,
                span: Span::new(0, 0, 0, 0),
            })],
        };

        let imports = collect_imports(&program);
        assert_eq!(imports.len(), 1);
        assert!(imports.contains_key("Vec3"));
        let (path, name) = &imports["Vec3"];
        assert_eq!(path, &["math"]);
        assert_eq!(name, "Vec3");
    }

    #[test]
    fn test_collect_imports_alias() {
        let program = Program {
            items: vec![Item::Use(UsePath {
                segments: vec!["math".into(), "Vector3".into()],
                alias: Some("V3".into()),
                glob: false,
                group: None,
                span: Span::new(0, 0, 0, 0),
            })],
        };

        let imports = collect_imports(&program);
        assert_eq!(imports.len(), 1);
        assert!(imports.contains_key("V3"));
    }

    #[test]
    fn test_collect_imports_group() {
        let program = Program {
            items: vec![Item::Use(UsePath {
                segments: vec!["math".into()],
                alias: None,
                glob: false,
                group: Some(vec![
                    UseGroupItem {
                        name: "Vec3".into(),
                        alias: None,
                    },
                    UseGroupItem {
                        name: "Mat4".into(),
                        alias: Some("Matrix".into()),
                    },
                ]),
                span: Span::new(0, 0, 0, 0),
            })],
        };

        let imports = collect_imports(&program);
        assert_eq!(imports.len(), 2);
        assert!(imports.contains_key("Vec3"));
        assert!(imports.contains_key("Matrix"));
    }
}
