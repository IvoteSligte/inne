use std::collections::HashMap;

use crate::{Span, Spanned, ast};

pub use crate::ast::{AssignOp, BinaryOp, UnaryOp};

pub type DefId = usize;

pub const UNKNOWN_DEF_ID: DefId = DefId::MAX;
pub const PRINT_DEF_ID: DefId = DefId::MAX - 1;

pub struct DefInfo {
    pub span: Span,
}

pub struct Hir<'a> {
    /// Essentially `HashMap<DefId, DefInfo>`, but all [DefId]`s are consecutive.
    pub(crate) defs: Vec<DefInfo>,
    pub(crate) funcs: Vec<Func<'a>>,
    pub(crate) stmts: Vec<Stmt<'a>>,
}

impl<'a> Hir<'a> {
    pub fn get_def_info(&self, def_id: DefId) -> &DefInfo {
        debug_assert_ne!(def_id, UNKNOWN_DEF_ID);
        debug_assert_ne!(def_id, PRINT_DEF_ID);
        &self.defs[def_id]
    }
}

pub enum Stmt<'a> {
    VarDecl(VarDecl<'a>),
    Expr(Spanned<Expr<'a>>),
    Assign(Assign<'a>),
}

pub type Block<'a> = Vec<Stmt<'a>>;

pub struct Func<'a> {
    pub def_id: DefId,
    pub name: Spanned<&'a str>,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Spanned<Type>>,
    pub body: Block<'a>,
}

pub struct Param<'a> {
    pub def_id: DefId,
    pub name: Spanned<&'a str>,
    pub r#type: Spanned<Type>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Nothing,
    Int,
    Bool,
    String,
    /// Typed HIR-only
    Function,
    Named(DefId),
}

impl Type {
    pub const UNKNOWN: Self = Self::Named(UNKNOWN_DEF_ID);

    pub fn is_known(self) -> bool {
        self != Type::UNKNOWN
    }

    pub fn is_unknown(self) -> bool {
        self == Type::UNKNOWN
    }

    pub fn or(self, other: Self) -> Type {
        if self == Type::UNKNOWN { other } else { self }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Type::Nothing => "nothing",
            Type::Int => "int",
            Type::Bool => "bool",
            Type::String => "string",
            Type::Function => "function",
            Type::Named(def_id) => return write!(f, "{def_id}"),
        })
    }
}

pub struct VarDecl<'a> {
    pub def_id: DefId,
    pub name: Spanned<&'a str>,
    pub value: Spanned<Expr<'a>>,
}

pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    Var(DefId),
    Int(&'a str),
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
    pub name: Spanned<&'a str>,
    pub def_id: DefId,
    pub args: Vec<Spanned<Expr<'a>>>,
}

pub struct Var<'a> {
    pub name: &'a str,
    pub def_id: DefId,
}

pub struct Assign<'a> {
    pub op: Spanned<AssignOp>,
    pub target: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

pub struct Error<'a> {
    pub filename: &'a str,
    pub message: String,
    pub span: Span,
    pub contexts: Vec<(String, Span)>,
}

pub fn hir_from_ast<'a>(
    ast: &'a ast::Ast<'a>,
    filename: &'a str,
) -> Result<Hir<'a>, Vec<Error<'a>>> {
    let constructor = Constructor::new(filename);
    constructor.map_ast(ast)
}

/// Helper type for managing error messages.
pub(crate) struct Errors<'a> {
    filename: &'a str,
    inner: Vec<Error<'a>>,
}

impl<'a> Errors<'a> {
    pub fn new(filename: &'a str) -> Self {
        Self {
            filename,
            inner: Vec::new(),
        }
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
            filename: self.filename,
            message: message.to_string(),
            span,
            contexts,
        });
    }

    pub fn into_inner(self) -> Vec<Error<'a>> {
        self.inner
    }
}

/// Using a separate type instead of having the fields and methods directly on [Constructor]
/// to prevent lifetime issues.
struct ConstructorDefs(Vec<DefInfo>);

impl ConstructorDefs {
    pub fn add(&mut self, span: Span) -> DefId {
        let id = self.0.len();
        self.0.push(DefInfo { span });
        id
    }

    fn get(&self, def_id: DefId) -> &DefInfo {
        debug_assert_ne!(def_id, UNKNOWN_DEF_ID);
        debug_assert_ne!(def_id, PRINT_DEF_ID);
        &self.0[def_id]
    }
}

struct Constructor<'a> {
    defs: ConstructorDefs,
    funcs: Vec<Func<'a>>,
    func_to_def_id: HashMap<&'a str, DefId>,
    scopes: Vec<HashMap<&'a str, DefId>>,
    errors: Errors<'a>,
}

impl<'a> Constructor<'a> {
    pub fn new(filename: &'a str) -> Self {
        Self {
            defs: ConstructorDefs(Vec::new()),
            funcs: Vec::new(),
            func_to_def_id: HashMap::new(),
            scopes: Vec::new(),
            errors: Errors::new(filename),
        }
    }

    pub fn map_ast(mut self, ast: &'a ast::Ast<'a>) -> Result<Hir<'a>, Vec<Error<'a>>> {
        let stmts = self.map_scope(ast);
        match self.errors.is_empty() {
            true => Ok(Hir {
                defs: self.defs.0,
                funcs: self.funcs,
                stmts,
            }),
            false => Err(self.errors.into_inner()),
        }
    }

    fn map_scope<'b>(&mut self, stmts: &[ast::Stmt<'b>]) -> Vec<Stmt<'b>>
    where
        'b: 'a,
    {
        let is_top_level = self.scopes.is_empty();
        if is_top_level {
            for stmt in stmts {
                let ast::Stmt::Func(func) = stmt else {
                    continue;
                };
                if let Some(&prev_func_def_id) = self.func_to_def_id.get(func.name.0) {
                    self.errors.add_with_contexts(
                        format!(
                            "Function '{}' was already defined. Functions must have unique names.",
                            func.name.0
                        ),
                        func.name.1,
                        vec![(
                            format!("Previously defined here:"),
                            self.defs.get(prev_func_def_id).span,
                        )],
                    );
                } else {
                    self.func_to_def_id
                        .insert(func.name.0, self.defs.add(func.name.1));
                }
            }
        }

        // NOTE: The scope always needs to be pushed at the end of this function,
        //       so do not use `return`.
        self.scopes.push(HashMap::new());

        let mut out_stmts = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            match stmt {
                ast::Stmt::Func(func) => {
                    if !is_top_level {
                        // TODO: add outer function as context
                        self.errors.add(
                            "Functions cannot not be defined inside other functions.",
                            func.name.1,
                        );
                        continue;
                    }
                    let def_id = *self.func_to_def_id.get(func.name.0).unwrap();
                    let params = func
                        .params
                        .iter()
                        .map(|param| self.map_param(param))
                        .collect();
                    let return_type = func.return_type.map(|r#type| self.map_type(&r#type));
                    let body = self.map_scope(&func.body);
                    self.funcs.push(Func {
                        def_id,
                        name: func.name,
                        params,
                        return_type,
                        body,
                    });
                }
                ast::Stmt::VarDecl(var_decl) => {
                    let value = self.map_expr(&var_decl.value);
                    // The declaration should be added *after* the value is analyzed,
                    // to prevent cyclic variable definitions.
                    let def_id = self.defs.add(var_decl.name.1);
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(var_decl.name.0, def_id);
                    out_stmts.push(Stmt::VarDecl(VarDecl {
                        def_id,
                        name: var_decl.name,
                        value,
                    }));
                }
                ast::Stmt::Expr(expr) => {
                    out_stmts.push(Stmt::Expr(self.map_expr(expr)));
                }
                ast::Stmt::Assign(assign) => {
                    out_stmts.push(Stmt::Assign(Assign {
                        op: assign.op,
                        target: self.map_expr(&assign.target),
                        value: self.map_expr(&assign.value),
                    }));
                }
            }
        }
        self.scopes.pop();
        out_stmts
    }

    fn map_param<'b>(&mut self, param: &ast::Param<'b>) -> Param<'b>
    where
        'b: 'a,
    {
        let def_id = self.defs.add(param.name.1);
        if let Some(prev_param_def_id) =
            self.scopes.last_mut().unwrap().insert(param.name.0, def_id)
        {
            self.errors.add_with_contexts(
                format!(
                    "Parameter '{}' was already defined. Parameters must have unique names.",
                    param.name.0
                ),
                param.name.1,
                vec![(
                    format!("Previously defined here:"),
                    self.defs.get(prev_param_def_id).span,
                )],
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
                ast::Expr::Call(call) => {
                    let def_id = match self.func_to_def_id.get(call.name.0) {
                        Some(&def_id) => def_id,
                        None if call.name.0 == "print" => PRINT_DEF_ID,
                        None => {
                            // TODO: add context if function is defined as variable
                            // TODO: add similarly named functions as context
                            self.errors
                                .add(format!("Unknown function '{}'", call.name.0), call.name.1);
                            UNKNOWN_DEF_ID
                        }
                    };
                    Expr::Call(Call {
                        name: call.name,
                        def_id,
                        args: call.args.iter().map(|arg| self.map_expr(arg)).collect(),
                    })
                }
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
                ast::Expr::String(string) => Expr::String(string),
            },
            *span,
        )
    }

    fn map_type(&mut self, (r#type, span): &Spanned<ast::Type<'_>>) -> Spanned<Type> {
        (
            match r#type {
                ast::Type::Nothing => Type::Nothing,
                ast::Type::Int => Type::Int,
                ast::Type::Bool => Type::Bool,
                ast::Type::String => Type::String,
                ast::Type::Named(name) => {
                    self.errors.add(format!("Unknown type '{}'", name), *span);
                    Type::Named(UNKNOWN_DEF_ID)
                }
            },
            *span,
        )
    }
}
