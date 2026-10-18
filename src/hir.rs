use std::collections::HashMap;

use crate::{Error, Span, Spanned, ast};

pub use crate::ast::{AssignOp, BinaryOp, UnaryOp};

// TODO: warn if expression used as statement does not have any side-effects
// TODO: warn for unused expression return value
// TODO: info for variable accessed from a different scope

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
    // Float
    Bool,
    String,
    /// Typed HIR-only
    Function,
    Range,
    // List(TypeId)
    // Map { key: TypeId, value: TypeId }
    // Tuple
    Named(DefId), // TODO: TypeId instead of DefId
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
            Type::Range => "range",
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
    pub name: Spanned<&'a str>,
    pub def_id: DefId,
    pub args: Vec<Spanned<Expr<'a>>>,
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
    pub var: Spanned<&'a str>,
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

    pub fn into_inner(self) -> Vec<Error> {
        self.inner
    }
}

/// Using a separate type instead of having the fields and methods directly on [Constructor]
/// to prevent lifetime issues.
struct ConstructorDefs<'a>(Vec<DefInfo<'a>>);

impl<'a> ConstructorDefs<'a> {
    pub fn add(&mut self, (name, span): Spanned<&'a str>) -> DefId {
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
    // TODO: separate VarId and FuncId
    func_to_def_id: HashMap<&'a str, DefId>,
    scopes: Vec<HashMap<&'a str, DefId>>,
    errors: Errors,
}

impl<'a> Constructor<'a> {
    pub fn map_ast(ast: &'a ast::Ast<'a>) -> Result<Hir<'a>, Vec<Error>> {
        let mut _self = Self {
            defs: ConstructorDefs(Vec::new()),
            funcs: Vec::new(),
            func_to_def_id: HashMap::new(),
            scopes: Vec::new(),
            errors: Errors::new(),
        };
        for stmt in &ast.stmts {
            let ast::Stmt::Func(func) = stmt else {
                continue;
            };
            if let Some(&prev_func_def_id) = _self.func_to_def_id.get(func.name.0) {
                _self.errors.add_with_contexts(
                    format!(
                        "Function '{}' was already defined. Functions must have unique names.",
                        func.name.0
                    ),
                    func.name.1,
                    vec![(
                        format!("Previously defined here:"),
                        _self.defs.get(prev_func_def_id).span,
                    )],
                );
            } else {
                _self
                    .func_to_def_id
                    .insert(func.name.0, _self.defs.add(func.name));
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
                    let def_id = *self.func_to_def_id.get(name.0).unwrap();
                    self.in_scope(|_self| {
                        let params = params.iter().map(|param| _self.map_param(param)).collect();
                        let return_type = return_type.map(|r#type| _self.map_type(&r#type));
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
            Expr::Binary(Binary {
                op: (op, _),
                left,
                right,
            }) => write!(f, "({} {op} {})", left.0, right.0)?,
            Expr::Unary(Unary { op: (op, _), expr }) => write!(f, "({op}{})", expr.0)?,
            Expr::Call(Call {
                name: (name, _),
                def_id: _,
                args,
            }) => {
                write!(f, "{name}(")?;
                for (i, (arg, _)) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
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
