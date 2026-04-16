// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A complete intent program with applications, type definitions, and modules.
///
/// ```
/// # use intent::ast::*;
/// let prog = IntentProgram {
///     applications: vec![],
///     types: vec![],
///     modules: vec![],
/// };
/// assert!(prog.types.is_empty());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntentProgram {
    /// Application definitions.
    pub applications: Vec<Application>,
    /// User-defined types.
    pub types: Vec<TypeDef>,
    /// Module definitions.
    pub modules: Vec<Module>,
}

/// A high-level application with CLI args, environment vars, and an intent phrase.
///
/// ```
/// # use intent::ast::*;
/// let app = Application {
///     name: "demo".to_owned(),
///     args: ArgsDef::default(),
///     environment: vec![],
///     intent: "print hello world".to_owned(),
/// };
/// assert_eq!(app.name, "demo");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    /// Application name.
    pub name: String,
    /// CLI argument definitions.
    pub args: ArgsDef,
    /// Environment variable bindings.
    pub environment: Vec<EnvVar>,
    /// The intent phrase describing desired behavior.
    pub intent: String,
}

// ---------------------------------------------------------------------------
// CLI Argument Definitions
// ---------------------------------------------------------------------------

/// CLI argument definitions: optional verb, flags, and positionals.
///
/// ```
/// # use intent::ast::ArgsDef;
/// let args = ArgsDef::default();
/// assert!(args.verb.is_none());
/// assert!(args.flags.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArgsDef {
    /// Optional verb (subcommand-style first positional).
    pub verb: Option<String>,
    /// CLI flag definitions.
    pub flags: Vec<FlagDef>,
    /// Positional argument definitions.
    pub positionals: Vec<PositionalDef>,
}

/// A CLI flag definition: `--name`, `--name Type`, or `--name Type default val`.
///
/// ```
/// # use intent::ast::FlagDef;
/// let f = FlagDef {
///     long_name: "verbose".to_owned(),
///     default: None,
///     ty: None,
/// };
/// assert!(f.ty.is_none(), "boolean flag has no type");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagDef {
    /// The flag name (without `--` prefix).
    pub long_name: String,
    /// Default value (`None` means required).
    pub default: Option<String>,
    /// The type (`None` for boolean flags).
    pub ty: Option<String>,
}

/// A positional argument definition: `binding Type`.
///
/// ```
/// # use intent::ast::PositionalDef;
/// let p = PositionalDef {
///     binding: "file".to_owned(),
///     ty: "String".to_owned(),
/// };
/// assert_eq!(p.ty, "String");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionalDef {
    /// The variable name in generated code.
    pub binding: String,
    /// The Synapse type name.
    pub ty: String,
}

/// An environment variable binding: `Type binding from VAR_NAME [default val]`.
///
/// ```
/// # use intent::ast::EnvVar;
/// let e = EnvVar {
///     binding: "api_key".to_owned(),
///     default: None,
///     ty: "String".to_owned(),
///     var_name: "API_KEY".to_owned(),
/// };
/// assert_eq!(e.var_name, "API_KEY");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    /// The variable name in generated code.
    pub binding: String,
    /// Default value (`None` means required).
    pub default: Option<String>,
    /// The Synapse type name.
    pub ty: String,
    /// The OS environment variable name.
    pub var_name: String,
}

/// A user-defined record type with named fields.
///
/// ```
/// # use intent::ast::*;
/// let td = TypeDef {
///     name: "Pair".to_owned(),
///     fields: vec![
///         Field {
///             name: "first".to_owned(),
///             ty: "Int".to_owned(),
///         },
///         Field {
///             name: "second".to_owned(),
///             ty: "Int".to_owned(),
///         },
///     ],
/// };
/// assert_eq!(td.fields.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    /// The type name.
    pub name: String,
    /// Record fields.
    pub fields: Vec<Field>,
}

// ---------------------------------------------------------------------------
// Type Definition Components
// ---------------------------------------------------------------------------

/// A field within a [`TypeDef`].
///
/// ```
/// # use intent::ast::Field;
/// let f = Field {
///     name: "x".to_owned(),
///     ty: "Int".to_owned(),
/// };
/// assert_eq!(f.ty, "Int");
/// ```
///
/// [`TypeDef`]: crate::ast::TypeDef
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The field name.
    pub name: String,
    /// The field type as a string.
    pub ty: String,
}

/// A named module containing capabilities and pipelines.
///
/// ```
/// # use intent::ast::Module;
/// let m = Module {
///     name: "math".to_owned(),
///     capabilities: vec![],
///     pipelines: vec![],
/// };
/// assert_eq!(m.name, "math");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// The module name.
    pub name: String,
    /// Capabilities defined in this module.
    pub capabilities: Vec<Capability>,
    /// Pipelines defined in this module.
    pub pipelines: Vec<Pipeline>,
}

// ---------------------------------------------------------------------------
// Module Components
// ---------------------------------------------------------------------------

/// A capability with typed inputs, output, and an intent phrase.
///
/// ```
/// # use intent::ast::*;
/// let cap = Capability {
///     name: "factorial".to_owned(),
///     inputs: vec![Param {
///         name: "n".to_owned(),
///         ty: "Int".to_owned(),
///     }],
///     intent: "compute factorial using recursion".to_owned(),
///     output: Some("Int".to_owned()),
/// };
/// assert_eq!(cap.inputs.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    /// The capability name.
    pub name: String,
    /// Typed input parameters.
    pub inputs: Vec<Param>,
    /// The intent phrase describing desired behavior.
    pub intent: String,
    /// The output type (if any).
    pub output: Option<String>,
}

/// A typed parameter for a [`Capability`].
///
/// ```
/// # use intent::ast::Param;
/// let p = Param {
///     name: "n".to_owned(),
///     ty: "Int".to_owned(),
/// };
/// assert_eq!(p.name, "n");
/// ```
///
/// [`Capability`]: crate::ast::Capability
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// The parameter name.
    pub name: String,
    /// The type as a string.
    pub ty: String,
}

/// A pipeline that chains capability invocations.
///
/// ```
/// # use intent::ast::*;
/// let pipe = Pipeline {
///     name: "pipe".to_owned(),
///     steps: vec![PipelineStep {
///         capability: "step_one".to_owned(),
///         args: vec!["x".to_owned()],
///     }],
/// };
/// assert_eq!(pipe.steps.len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    /// The pipeline name.
    pub name: String,
    /// Ordered steps to execute.
    pub steps: Vec<PipelineStep>,
}

/// A single step in a [`Pipeline`].
///
/// ```
/// # use intent::ast::PipelineStep;
/// let step = PipelineStep {
///     args: vec!["n".to_owned()],
///     capability: "factorial".to_owned(),
/// };
/// assert_eq!(step.capability, "factorial");
/// ```
///
/// [`Pipeline`]: crate::ast::Pipeline
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineStep {
    /// Arguments to pass to the capability.
    pub args: Vec<String>,
    /// The capability to invoke.
    pub capability: String,
}
