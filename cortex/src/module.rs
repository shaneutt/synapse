use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    ast::Type,
    typed_ast::{TypedDeclaration, TypedProgram},
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Describes one function's public signature within a module.
///
/// ```
/// # use cortex::{ast::Type, module::FunctionSig};
/// let sig = FunctionSig {
///     name: "add".to_owned(),
///     params: vec![("a".to_owned(), Type::Int), ("b".to_owned(), Type::Int)],
///     return_type: Type::Int,
/// };
/// assert_eq!(sig.params.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSig {
    /// The function name.
    pub name: String,
    /// Parameter names and types.
    pub params: Vec<(String, Type)>,
    /// The return type.
    pub return_type: Type,
}

/// The public API surface of a compiled Synapse module.
///
/// ```
/// # use cortex::module::ModuleApi;
/// let api = ModuleApi {
///     name: "math".to_owned(),
///     functions: vec![],
/// };
/// assert!(api.functions.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleApi {
    /// The module name (file stem).
    pub name: String,
    /// Public function signatures exported by this module.
    pub functions: Vec<FunctionSig>,
}

/// Scans a directory for `.synapse` files and returns `(module_name, path)` pairs.
///
/// The module name is the file stem. Files are returned in sorted order
/// for deterministic builds.
///
/// ```no_run
/// # use cortex::module::discover_modules;
/// let modules = discover_modules(std::path::Path::new("src"));
/// for (name, path) in &modules {
///     println!("{name}: {}", path.display());
/// }
/// ```
pub fn discover_modules(dir: &Path) -> Vec<(String, PathBuf)> {
    tracing::debug!(dir = %dir.display(), "discovering synapse modules");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut modules: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "synapse") {
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                Some((stem, path))
            } else {
                None
            }
        })
        .collect();

    modules.sort_by(|a, b| a.0.cmp(&b.0));
    modules
}

/// Extracts the public API from a type-checked program.
///
/// Iterates all declarations and collects signatures for `pub` functions.
///
/// ```
/// # use cortex::{module::extract_api, typed_ast::*, ast::*, token::Span};
/// let span = Span {
///     line: 1,
///     column: 1,
///     length: 1,
/// };
/// let program = TypedProgram {
///     declarations: vec![
///         TypedDeclaration::Function(TypedFunction {
///             name: "add".to_owned(),
///             body: vec![TypedStatement::Returns(TypedExpr {
///                 kind: TypedExprKind::IntLit(0),
///                 span,
///                 ty: Type::Int,
///             })],
///             is_public: true,
///             params: vec![TypedParam {
///                 name: "x".to_owned(),
///                 span,
///                 ty: Type::Int,
///             }],
///             return_type: Type::Int,
///             span,
///         }),
///         TypedDeclaration::Function(TypedFunction {
///             name: "helper".to_owned(),
///             body: vec![TypedStatement::Returns(TypedExpr {
///                 kind: TypedExprKind::IntLit(0),
///                 span,
///                 ty: Type::Int,
///             })],
///             is_public: false,
///             params: vec![],
///             return_type: Type::Int,
///             span,
///         }),
///     ],
///     imports: vec![],
/// };
/// let api = extract_api("mymod", &program);
/// assert_eq!(api.functions.len(), 1);
/// assert_eq!(api.functions[0].name, "add");
/// ```
pub fn extract_api(module_name: &str, program: &TypedProgram) -> ModuleApi {
    tracing::debug!(module = module_name, "extracting module API");
    let functions = program
        .declarations
        .iter()
        .filter_map(|decl| {
            if let TypedDeclaration::Function(f) = decl {
                if f.is_public {
                    let params = f.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
                    return Some(FunctionSig {
                        name: f.name.clone(),
                        params,
                        return_type: f.return_type.clone(),
                    });
                }
            }
            None
        })
        .collect();

    ModuleApi {
        name: module_name.to_owned(),
        functions,
    }
}

/// Convenience alias for a map of available module APIs keyed by name.
pub type ModuleMap = HashMap<String, ModuleApi>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ast::Type,
        token::Span,
        typed_ast::{
            TypedDeclaration, TypedExpr, TypedExprKind, TypedFunction, TypedParam, TypedProgram, TypedStatement,
        },
    };

    #[test]
    fn discover_modules_finds_synapse_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("math.synapse"), "").unwrap();
        std::fs::write(dir.path().join("utils.synapse"), "").unwrap();
        std::fs::write(dir.path().join("readme.txt"), "").unwrap();

        let modules = discover_modules(dir.path());
        assert_eq!(modules.len(), 2, "should find exactly two .synapse files");
        assert_eq!(modules[0].0, "math");
        assert_eq!(modules[1].0, "utils");
    }

    #[test]
    fn discover_modules_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let modules = discover_modules(dir.path());
        assert!(modules.is_empty(), "empty dir yields no modules");
    }

    #[test]
    fn discover_modules_nonexistent_dir() {
        let modules = discover_modules(Path::new("/nonexistent/path"));
        assert!(modules.is_empty(), "nonexistent dir yields no modules");
    }

    #[test]
    fn extract_api_pub_functions_only() {
        let span = Span {
            line: 1,
            column: 1,
            length: 1,
        };
        let program = TypedProgram {
            declarations: vec![
                TypedDeclaration::Function(TypedFunction {
                    name: "public_fn".to_owned(),
                    body: vec![TypedStatement::Returns(TypedExpr {
                        kind: TypedExprKind::IntLit(0),
                        span,
                        ty: Type::Int,
                    })],
                    is_public: true,
                    params: vec![
                        TypedParam {
                            name: "a".to_owned(),
                            span,
                            ty: Type::Int,
                        },
                        TypedParam {
                            name: "b".to_owned(),
                            span,
                            ty: Type::Str,
                        },
                    ],
                    return_type: Type::Bool,
                    span,
                }),
                TypedDeclaration::Function(TypedFunction {
                    name: "private_fn".to_owned(),
                    body: vec![TypedStatement::Returns(TypedExpr {
                        kind: TypedExprKind::IntLit(0),
                        span,
                        ty: Type::Int,
                    })],
                    is_public: false,
                    params: vec![],
                    return_type: Type::Int,
                    span,
                }),
            ],
            imports: vec![],
        };

        let api = extract_api("test_mod", &program);
        assert_eq!(api.name, "test_mod");
        assert_eq!(api.functions.len(), 1, "only pub functions are extracted");
        assert_eq!(api.functions[0].name, "public_fn");
        assert_eq!(api.functions[0].params.len(), 2);
        assert_eq!(api.functions[0].params[0], ("a".to_owned(), Type::Int));
        assert_eq!(api.functions[0].params[1], ("b".to_owned(), Type::Str));
        assert_eq!(api.functions[0].return_type, Type::Bool);
    }

    #[test]
    fn extract_api_empty_program() {
        let api = extract_api(
            "empty",
            &TypedProgram {
                declarations: vec![],
                imports: vec![],
            },
        );
        assert!(api.functions.is_empty(), "no declarations yields no functions");
    }
}
