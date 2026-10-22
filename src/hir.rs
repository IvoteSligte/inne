use std::collections::HashMap;

use crate::{Error, Span, Spanned, ast, util::write_comma_separated};

pub use crate::ast::{AssignOp, BinaryOp, Name, UnaryOp};

// TODO: warn if expression used as statement does not have any side-effects
// TODO: warn for unused expression return value
// TODO: info for variable accessed from a different scope

// TODO: newtype instead of alias
pub type DefId = usize;

pub const UNKNOWN_DEF_ID: DefId = DefId::MAX;
pub const PRINT_DEF_ID: DefId = DefId::MAX - 1;

pub struct DefInfo<'a> {
    pub name: &'a str,
    pub span: Span,
}

pub struct Hir<'a> {
    pub(crate) filename: &'a str,
    /// Essentially `HashMap<DefId, DefInfo>`, but all [DefId]`s are consecutive.
    pub(crate) defs: Vec<DefInfo<'a>>,
    pub(crate) funcs: Vec<Func<'a>>,
    pub(crate) types: Vec<Struct<'a>>,
    pub(crate) stmts: Vec<Stmt<'a>>,
}

impl<'a> Hir<'a> {
    pub fn get_def_info(&self, def_id: DefId) -> &DefInfo<'a> {
        debug_assert_ne!(def_id, UNKNOWN_DEF_ID);
        debug_assert_ne!(def_id, PRINT_DEF_ID);
        &self.defs[def_id]
    }
}

pub enum Stmt<'a> {
    VarDecl(VarDecl<'a>),
    Expr(Spanned<Expr<'a>>),
    Assign(Assign<'a>),
    If(If<'a>),
    For(For<'a>),
    While(While<'a>),
    Loop(Loop<'a>),
    Break(Span),
    Continue(Span),
    Return(Spanned<Option<Spanned<Expr<'a>>>>),
}

pub type Block<'a> = Vec<Stmt<'a>>;

pub struct Func<'a> {
    pub def_id: DefId,
    pub name: Name<'a>,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Spanned<Type>>,
    pub body: Block<'a>,
}

pub struct Param<'a> {
    pub def_id: DefId,
    pub name: Name<'a>,
    pub r#type: Spanned<Type>,
}

pub struct Struct<'a> {
    pub type_id: TypeId,
    pub name: Name<'a>,
    pub fields: Vec<Field<'a>>,
}

pub struct Field<'a> {
    pub name: Name<'a>,
    pub r#type: Spanned<Type>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypeId(usize);

impl TypeId {
    pub const UNKNOWN: Self = Self(usize::MAX);

    pub fn get(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for TypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// TODO: make cheaper to clone with Rc's
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Nothing,
    Int,
    // Float
    Bool,
    String,
    Char,
    /// Typed-HIR-only
    Function,
    Range,
    List(Box<Self>),
    Map {
        key: Box<Self>,
        value: Box<Self>,
    },
    // Tuple
    Named {
        id: TypeId,
        generics: Vec<Self>,
    },
}

impl Type {
    pub const UNKNOWN: Self = Self::Named {
        id: TypeId::UNKNOWN,
        generics: Vec::new(),
    };

    pub fn is_known(&self) -> bool {
        *self != Type::UNKNOWN
    }

    pub fn is_unknown(&self) -> bool {
        *self == Type::UNKNOWN
    }

    pub fn or<'a>(&'a self, other: &'a Self) -> &'a Type {
        if *self == Type::UNKNOWN { other } else { self }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Type::Nothing => "nothing",
            Type::Int => "int",
            Type::Bool => "bool",
            Type::String => "string",
            Type::Char => "char",
            Type::Function => "function",
            Type::Range => "range",
            Type::List(element) => return write!(f, "List<{element}>"),
            Type::Map { key, value } => return write!(f, "Map<{key}, {value}>"),
            Type::Named { id, generics } => {
                write!(f, "{id}")?;
                if !generics.is_empty() {
                    f.write_str("<")?;
                    write_comma_separated(f, generics)?;
                    f.write_str(">")?;
                }
                return Ok(());
            }
        })
    }
}

pub struct VarDecl<'a> {
    pub def_id: DefId,
    pub name: Name<'a>,
    pub value: Spanned<Expr<'a>>,
}

pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    EmptyList(EmptyList),
    List(List<'a>),
    EmptyMap(EmptyMap),
    Map(Map<'a>),
    Index(Index<'a>),
    Range(Range<'a>),
    Var(DefId),
    Int(&'a str),
    Bool(bool),
    String(&'a str),
}

pub struct Binary<'a> {
    pub op: Spanned<BinaryOp>,
    pub left: Box<Spanned<Expr<'a>>>,
    pub right: Box<Spanned<Expr<'a>>>,
}

pub struct Unary<'a> {
    pub op: Spanned<UnaryOp>,
    pub expr: Box<Spanned<Expr<'a>>>,
}

pub struct Call<'a> {
    pub name: Name<'a>,
    pub def_id: DefId,
    pub args: Vec<Spanned<Expr<'a>>>,
}

pub struct EmptyList {
    pub element_type: Spanned<Type>,
}

pub struct List<'a> {
    pub elements: Vec<Spanned<Expr<'a>>>,
}
pub struct EmptyMap {
    pub key_type: Spanned<Type>,
    pub value_type: Spanned<Type>,
}

pub struct Map<'a> {
    pub pairs: Vec<Pair<'a>>,
}

pub struct Pair<'a> {
    pub key: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

pub struct Index<'a> {
    pub expr: Box<Spanned<Expr<'a>>>,
    pub index: Box<Spanned<Expr<'a>>>,
}

pub struct Range<'a> {
    pub start: Box<Spanned<Expr<'a>>>,
    pub end: Box<Spanned<Expr<'a>>>,
}

pub struct Assign<'a> {
    pub op: Spanned<AssignOp>,
    pub target: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

pub struct If<'a> {
    /// Conditions and associated blocks to be evaluated in order.
    /// Originally an if-else chain.
    pub choices: Vec<Choice<'a>>,
    pub r#else: Option<Block<'a>>,
}

pub struct Choice<'a> {
    pub cond: Spanned<Expr<'a>>,
    pub then: Block<'a>,
}

pub struct For<'a> {
    /// ID of the for-loop iteration variable.
    pub def_id: DefId,
    pub var: Name<'a>,
    pub iter: Spanned<Expr<'a>>,
    pub body: Block<'a>,
}

pub struct While<'a> {
    pub cond: Spanned<Expr<'a>>,
    pub body: Block<'a>,
}

pub struct Loop<'a> {
    pub body: Block<'a>,
}

// TODO: return (Option<Hir<'a>>, Vec<Error>) instead because the Hir may be recoverable on error
pub fn hir_from_ast<'a>(ast: &'a ast::Ast<'a>) -> Result<Hir<'a>, Vec<Error>> {
    Constructor::map_ast(ast)
}

/// Helper type for managing error messages.
pub(crate) struct Errors {
    inner: Vec<Error>,
}

impl Errors {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn add(&mut self, message: impl ToString, span: Span) {
        self.add_with_contexts(message, span, Vec::new());
    }

    pub fn add_with_contexts(
        &mut self,
        message: impl ToString,
        span: Span,
        contexts: Vec<(String, Span)>,
    ) {
        self.inner.push(Error {
            message: message.to_string(),
            span,
            contexts,
        });
    }

    pub fn add_with_context(
        &mut self,
        message: impl ToString,
        span: Span,
        context_message: impl ToString,
        context_span: Span,
    ) {
        self.add_with_contexts(
            message,
            span,
            vec![(context_message.to_string(), context_span)],
        );
    }

    pub fn into_inner(self) -> Vec<Error> {
        self.inner
    }
}

/// Using a separate type instead of having the fields and methods directly on [Constructor]
/// to prevent lifetime issues.
struct ConstructorDefs<'a>(Vec<DefInfo<'a>>);

impl<'a> ConstructorDefs<'a> {
    pub fn add(&mut self, (name, span): Name<'a>) -> DefId {
        let id = self.0.len();
        self.0.push(DefInfo { name, span });
        id
    }

    fn get(&self, def_id: DefId) -> &DefInfo<'a> {
        debug_assert_ne!(def_id, UNKNOWN_DEF_ID);
        debug_assert_ne!(def_id, PRINT_DEF_ID);
        &self.0[def_id]
    }
}

struct Constructor<'a> {
    defs: ConstructorDefs<'a>,
    funcs: Vec<Func<'a>>,
    types: Vec<Struct<'a>>,
    // TODO: separate VarId and FuncId
    func_map: HashMap<&'a str, DefId>,
    type_map: HashMap<&'a str, (TypeId, Span)>,
    scopes: Vec<HashMap<&'a str, DefId>>,
    errors: Errors,
}

impl<'a> Constructor<'a> {
    pub fn map_ast(ast: &'a ast::Ast<'a>) -> Result<Hir<'a>, Vec<Error>> {
        let mut _self = Self {
            defs: ConstructorDefs(Vec::new()),
            funcs: Vec::new(),
            types: Vec::new(),
            func_map: HashMap::new(),
            type_map: HashMap::new(),
            scopes: Vec::new(),
            errors: Errors::new(),
        };
        for stmt in &ast.stmts {
            match stmt {
                ast::Stmt::Func(func) => {
                    let name = func.name;
                    if let Some(&prev_func_def_id) = _self.func_map.get(name.0) {
                        _self.errors.add_with_context(
                            format!(
                                "Function '{}' was already defined. Functions must have unique names.",
                                name.0
                            ),
                            name.1,
                            "Previously defined here:",
                            _self.defs.get(prev_func_def_id).span,
                        );
                    } else {
                        _self.func_map.insert(name.0, _self.defs.add(name));
                    }
                }
                ast::Stmt::Struct(r#struct) => {
                    let name = r#struct.name;
                    // TODO: error if user defines type with same name as builtin
                    //       (unlikely because user-defined types should be PascalCase by convention)
                    if let Some(&(_, prev_type_span)) = _self.type_map.get(name.0) {
                        _self.errors.add_with_context(
                            format!(
                                "Type '{}' was already defined. Types must have unique names.",
                                name.0
                            ),
                            name.1,
                            "Previously defined here:",
                            prev_type_span,
                        );
                    } else {
                        let id = TypeId(_self.type_map.len());
                        _self.type_map.insert(name.0, (id, name.1));
                    }
                }
                _ => {}
            }
        }
        let stmts = _self.map_scope(&ast.stmts);
        if !_self.errors.is_empty() {
            return Err(_self.errors.into_inner());
        }
        Ok(Hir {
            filename: ast.filename,
            defs: _self.defs.0,
            funcs: _self.funcs,
            types: _self.types,
            stmts,
        })
    }

    fn in_scope<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(HashMap::new());
        let value = f(self);
        self.scopes.pop();
        value
    }

    fn map_scope<'b>(&mut self, stmts: &[ast::Stmt<'b>]) -> Vec<Stmt<'b>>
    where
        'b: 'a,
    {
        self.in_scope(|_self| _self.map_stmts(stmts))
    }

    fn map_stmts<'b>(&mut self, stmts: &[ast::Stmt<'b>]) -> Vec<Stmt<'b>>
    where
        'b: 'a,
    {
        let mut out_stmts = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match stmt {
                ast::Stmt::Func(ast::Func {
                    name,
                    params,
                    return_type,
                    body,
                }) => {
                    if self.scopes.len() > 1 {
                        // TODO: add outer function as context
                        self.errors.add(
                            "Functions cannot not be defined inside other functions.",
                            name.1,
                        );
                        continue;
                    }
                    for (i, param) in params.iter().enumerate() {
                        for other_param in &params[i + 1..] {
                            if param.name.0 == other_param.name.0 {
                                self.errors.add("This parameter name was already used. Parameters must have unique names.", other_param.name.1);
                                break;
                            }
                        }
                    }
                    let def_id = *self.func_map.get(name.0).unwrap();
                    self.in_scope(|_self| {
                        let params = params.iter().map(|param| _self.map_param(param)).collect();
                        let return_type =
                            return_type.as_ref().map(|r#type| _self.map_type(&r#type));
                        let body = _self.map_stmts(&body);
                        _self.funcs.push(Func {
                            def_id,
                            name: *name,
                            params,
                            return_type,
                            body,
                        });
                    });
                }
                ast::Stmt::Struct(ast::Struct { name, fields }) => {
                    if self.scopes.len() > 1 {
                        // TODO: add outer function as context
                        self.errors
                            .add("Types cannot not be defined inside functions.", name.1);
                        continue;
                    }
                    for (i, field) in fields.iter().enumerate() {
                        for other_field in &fields[i + 1..] {
                            if field.name.0 == other_field.name.0 {
                                self.errors.add("This field name was already used. Fields must have unique names.", other_field.name.1);
                                break;
                            }
                        }
                    }
                    let (type_id, _) = *self.type_map.get(name.0).unwrap();
                    let fields = fields
                        .iter()
                        .map(|field| Field {
                            name: field.name,
                            r#type: self.map_type(&field.r#type),
                        })
                        .collect();
                    debug_assert_eq!(self.types.len(), type_id.get());
                    self.types.push(Struct {
                        type_id,
                        name: *name,
                        fields,
                    });
                }
                ast::Stmt::VarDecl(ast::VarDecl { name, value }) => {
                    let value = self.map_expr(value);
                    // The declaration should be added *after* the value is analyzed,
                    // to prevent cyclic variable definitions.
                    let def_id = self.defs.add(*name);
                    self.scopes.last_mut().unwrap().insert(name.0, def_id);
                    out_stmts.push(Stmt::VarDecl(VarDecl {
                        def_id,
                        name: *name,
                        value,
                    }));
                }
                ast::Stmt::Expr(expr) => {
                    out_stmts.push(Stmt::Expr(self.map_expr(expr)));
                }
                ast::Stmt::Assign(ast::Assign { op, target, value }) => {
                    out_stmts.push(Stmt::Assign(Assign {
                        op: *op,
                        target: self.map_expr(target),
                        value: self.map_expr(value),
                    }));
                }
                ast::Stmt::If(ast::If {
                    cond,
                    then,
                    else_ifs,
                    r#else,
                }) => {
                    let mut choices = vec![Choice {
                        cond: self.map_expr(cond),
                        then: self.map_scope(then),
                    }];
                    choices.extend(else_ifs.iter().map(|(cond, then)| Choice {
                        cond: self.map_expr(cond),
                        then: self.map_scope(then),
                    }));
                    out_stmts.push(Stmt::If(If {
                        choices,
                        r#else: r#else.as_ref().map(|r#else| self.map_scope(r#else)),
                    }))
                }
                ast::Stmt::For(ast::For { var, iter, body }) => {
                    let def_id = self.defs.add(*var);
                    let iter = self.map_expr(iter);
                    self.in_scope(|_self| {
                        _self.scopes.last_mut().unwrap().insert(var.0, def_id);
                        let body = _self.map_stmts(body);
                        out_stmts.push(Stmt::For(For {
                            def_id,
                            var: *var,
                            iter,
                            body,
                        }));
                    });
                }
                ast::Stmt::While(ast::While { cond, body }) => out_stmts.push(Stmt::While(While {
                    cond: self.map_expr(cond),
                    body: self.map_scope(body),
                })),
                ast::Stmt::Loop(ast::Loop { body }) => out_stmts.push(Stmt::Loop(Loop {
                    body: self.map_scope(body),
                })),
                ast::Stmt::Break(span) => out_stmts.push(Stmt::Break(*span)),
                ast::Stmt::Continue(span) => out_stmts.push(Stmt::Continue(*span)),
                ast::Stmt::Return((value, span)) => {
                    out_stmts.push(Stmt::Return((
                        value.as_ref().map(|value| self.map_expr(value)),
                        *span,
                    )));
                }
            }
        }
        out_stmts
    }

    fn map_param<'b>(&mut self, param: &ast::Param<'b>) -> Param<'b>
    where
        'b: 'a,
    {
        let def_id = self.defs.add(param.name);
        if let Some(prev_param_def_id) =
            self.scopes.last_mut().unwrap().insert(param.name.0, def_id)
        {
            self.errors.add_with_context(
                format!(
                    "Parameter '{}' was already defined. Parameters must have unique names.",
                    param.name.0
                ),
                param.name.1,
                format!("Previously defined here:"),
                self.defs.get(prev_param_def_id).span,
            );
        }
        Param {
            def_id,
            name: param.name,
            r#type: self.map_type(&param.r#type),
        }
    }

    fn map_expr<'b>(&mut self, (expr, span): &Spanned<ast::Expr<'b>>) -> Spanned<Expr<'b>>
    where
        'b: 'a,
    {
        (
            match expr {
                ast::Expr::Binary(ast::Binary { op, left, right }) => Expr::Binary(Binary {
                    op: *op,
                    left: Box::new(self.map_expr(left)),
                    right: Box::new(self.map_expr(right)),
                }),
                ast::Expr::Unary(ast::Unary { op, expr }) => Expr::Unary(Unary {
                    op: *op,
                    expr: Box::new(self.map_expr(expr)),
                }),
                ast::Expr::Call(ast::Call { name, args }) => {
                    let def_id = match self.func_map.get(name.0) {
                        Some(&def_id) => def_id,
                        None if name.0 == "print" => PRINT_DEF_ID,
                        None => {
                            // TODO: add context if function is defined as variable
                            // TODO: add similarly named functions as context
                            self.errors
                                .add(format!("Unknown function '{}'", name.0), name.1);
                            UNKNOWN_DEF_ID
                        }
                    };
                    Expr::Call(Call {
                        name: *name,
                        def_id,
                        args: args.iter().map(|arg| self.map_expr(arg)).collect(),
                    })
                }
                ast::Expr::EmptyList(ast::EmptyList { element_type }) => {
                    Expr::EmptyList(EmptyList {
                        element_type: self.map_type(element_type),
                    })
                }
                ast::Expr::List(ast::List { elements }) => Expr::List(List {
                    elements: elements.iter().map(|elem| self.map_expr(elem)).collect(),
                }),
                ast::Expr::EmptyMap(ast::EmptyMap {
                    key_type,
                    value_type,
                }) => Expr::EmptyMap(EmptyMap {
                    key_type: self.map_type(key_type),
                    value_type: self.map_type(value_type),
                }),
                ast::Expr::Map(ast::Map { pairs }) => Expr::Map(Map {
                    pairs: pairs
                        .iter()
                        .map(|pair| Pair {
                            key: self.map_expr(&pair.key),
                            value: self.map_expr(&pair.value),
                        })
                        .collect(),
                }),
                ast::Expr::Index(ast::Index { expr, index }) => Expr::Index(Index {
                    expr: Box::new(self.map_expr(expr)),
                    index: Box::new(self.map_expr(index)),
                }),
                ast::Expr::Var(name) => {
                    match self.scopes.iter().rev().find_map(|scope| scope.get(name)) {
                        Some(&def_id) => Expr::Var(def_id),
                        None => {
                            // TODO: add context if variable is defined as function
                            // TODO: add similarly named variables as context
                            self.errors
                                .add(format!("Unknown variable '{}'", name), *span);
                            Expr::Var(UNKNOWN_DEF_ID)
                        }
                    }
                }
                ast::Expr::Int(int) => Expr::Int(int),
                ast::Expr::Bool(bool) => Expr::Bool(*bool),
                ast::Expr::String(string) => Expr::String(string),
                ast::Expr::Range(ast::Range { start, end }) => Expr::Range(Range {
                    start: Box::new(self.map_expr(start)),
                    end: Box::new(self.map_expr(end)),
                }),
            },
            *span,
        )
    }

    fn map_type(&mut self, (r#type, span): &Spanned<ast::Type<'_>>) -> Spanned<Type> {
        let ast::Type { name, generics } = r#type;
        if matches!(name.0, "int" | "char" | "string" | "bool") && !generics.is_empty() {
            self.errors
                .add(format!("'{}' takes no generic arguments", name.0), *span);
        }
        if name.0 == "list" && generics.len() != 1 {
            self.errors
                .add(format!("'list<Element>' takes one generic argument"), *span);
        }
        if name.0 == "map" && generics.len() != 2 {
            self.errors.add(
                format!("'map<Key, Value>' takes two generic arguments"),
                *span,
            );
        }
        let r#type = match name.0 {
            "int" => Type::Int,
            "string" => Type::String,
            "bool" => Type::Bool,
            "char" => Type::Char,
            "list" => {
                let element = generics
                    .get(0)
                    .map(|r#type| self.map_type(r#type).0)
                    .unwrap_or(Type::UNKNOWN);
                Type::List(Box::new(element))
            }
            "map" => {
                let key = generics
                    .get(0)
                    .map(|r#type| self.map_type(r#type).0)
                    .unwrap_or(Type::UNKNOWN);
                let value = generics
                    .get(1)
                    .map(|r#type| self.map_type(r#type).0)
                    .unwrap_or(Type::UNKNOWN);
                if generics.len() != 2 {
                    self.errors.add(
                        format!(
                            "'map' takes two generic types, but {} were provided. Example: map<string, int>",
                            generics.len()
                        ),
                        *span,
                    );
                }
                Type::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                }
            }
            _ => {
                let generics = generics
                    .iter()
                    .map(|generic| self.map_type(generic).0)
                    .collect();
                let id = match self.type_map.get(name.0) {
                    Some((type_id, _)) => *type_id,
                    None => {
                        self.errors
                            .add(format!("Unknown type '{}'", name.0), name.1);
                        TypeId::UNKNOWN
                    }
                };
                Type::Named { id, generics }
            }
        };
        (r#type, *span)
    }
}

use indentify::*;

impl std::fmt::Display for Hir<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: Not sure why it doesn't work without the dyn conversion.
        let f: &mut dyn std::fmt::Write = &mut IndentWriter::new("    ", f);
        writeln!(f, "HIR '{}'", self.filename)?;
        writeln!(f, "defs: {{{INDENT}")?;
        for (i, DefInfo { name, span }) in self.defs.iter().enumerate() {
            writeln!(f, "_{i}: var {name} at {span};")?;
        }
        writeln!(f, "{DEDENT}}}")?;

        for Func {
            def_id,
            name: (name, _),
            params,
            return_type,
            body,
        } in &self.funcs
        {
            write!(f, "func _{} {name}(", def_id)?;
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "_{} {}: {}", param.def_id, param.name.0, param.r#type.0)?;
            }
            write!(f, ") ")?;
            if let Some((return_type, _)) = return_type {
                write!(f, "-> {return_type} ")?;
            }
            writeln!(f, "{{{INDENT}")?;
            for stmt in body {
                writeln!(f, "{stmt}")?;
            }
            writeln!(f, "{DEDENT}}}")?;
        }
        for stmt in &self.stmts {
            writeln!(f, "{stmt}")?;
        }
        Ok(())
    }
}

impl<'a> std::fmt::Display for Stmt<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stmt::VarDecl(VarDecl {
                def_id,
                name: (name, _),
                value: (value, _),
            }) => {
                write!(f, "var _{def_id} {name} = {value};")?;
            }
            Stmt::Expr((expr, _)) => write!(f, "{expr};")?,
            Stmt::Assign(Assign {
                op: (op, _),
                target: (target, _),
                value: (value, _),
            }) => {
                write!(f, "{target} {op} {value};")?;
            }
            Stmt::If(If { choices, r#else }) => {
                writeln!(f, "if {} {{{INDENT}", choices[0].cond.0)?;
                for stmt in &choices[0].then {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
                for choice in &choices[1..] {
                    writeln!(f, " else if {} {{{INDENT}", choice.cond.0)?;
                    for stmt in &choice.then {
                        writeln!(f, "{stmt}")?;
                    }
                    write!(f, "{DEDENT}}}")?;
                }
                if let Some(r#else) = r#else {
                    writeln!(f, " else {{{INDENT}")?;
                    for stmt in r#else {
                        writeln!(f, "{stmt}")?;
                    }
                    write!(f, "{DEDENT}}}")?;
                }
            }
            Stmt::For(For {
                def_id,
                var: (var, _),
                iter: (iter, _),
                body,
            }) => {
                writeln!(f, "for _{def_id} {var} in {iter} {{{INDENT}")?;
                for stmt in body {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
            }
            Stmt::While(While {
                cond: (cond, _),
                body,
            }) => {
                writeln!(f, "while {cond} {{{INDENT}")?;
                for stmt in body {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
            }
            Stmt::Loop(Loop { body }) => {
                writeln!(f, "loop {{{INDENT}")?;
                for stmt in body {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
            }
            Stmt::Break(_) => write!(f, "break;")?,
            Stmt::Continue(_) => write!(f, "continue;")?,
            Stmt::Return((value, _)) => {
                f.write_str("return")?;
                if let Some((value, _)) = value {
                    write!(f, " {value}")?;
                }
                f.write_str(";")?;
            }
        }
        Ok(())
    }
}

impl<'a> std::fmt::Display for Expr<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Binary(Binary { op, left, right }) => {
                write!(f, "({} {} {})", left.0, op.0, right.0)?
            }
            Expr::Unary(Unary { op, expr }) => write!(f, "({}{})", op.0, expr.0)?,
            Expr::Call(Call { name, args, .. }) => {
                write!(f, "{}(", name.0)?;
                write_comma_separated(f, args.iter().map(|(arg, _)| arg))?;
                f.write_str(")")?;
            }
            Expr::EmptyList(EmptyList { element_type }) => {
                f.write_str("<")?;
                element_type.0.fmt(f)?;
                f.write_str(">")?;
            }
            Expr::List(List { elements }) => {
                f.write_str("[")?;
                write_comma_separated(f, elements.iter().map(|(elem, _)| elem))?;
                f.write_str("]")?;
            }
            Expr::EmptyMap(EmptyMap {
                key_type,
                value_type,
            }) => {
                f.write_str("<")?;
                key_type.0.fmt(f)?;
                f.write_str(", ")?;
                value_type.0.fmt(f)?;
                f.write_str(">")?;
            }
            Expr::Map(Map { pairs }) => {
                f.write_str("{")?;
                write_comma_separated(f, pairs.iter())?;
                f.write_str("}")?;
            }
            Expr::Index(Index { expr, index }) => {
                f.write_str("(")?;
                expr.0.fmt(f)?;
                f.write_str("[")?;
                index.0.fmt(f)?;
                f.write_str("]")?;
                f.write_str(")")?;
            }
            Expr::Range(Range { start, end }) => write!(f, "{}..{}", start.0, end.0)?,
            Expr::Var(def_id) => write!(f, "_{def_id}")?,
            Expr::Int(int) => write!(f, "{int}")?,
            Expr::Bool(bool) => write!(f, "{bool}")?,
            Expr::String(string) => write!(f, "{string:?}")?,
        }
        Ok(())
    }
}

impl std::fmt::Display for Pair<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.key.0.fmt(f)?;
        f.write_str(": ")?;
        self.value.0.fmt(f)
    }
}
