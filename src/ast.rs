use crate::{ParseTree, Spanned, parser};

pub use crate::parser::{AssignOp, BinaryOp, UnaryOp};

pub struct Ast<'a> {
    pub filename: &'a str,
    pub stmts: Vec<Stmt<'a>>,
}

#[derive(Clone)]
pub enum Stmt<'a> {
    Func(Func<'a>),
    VarDecl(VarDecl<'a>),
    Expr(Spanned<Expr<'a>>),
    Assign(Assign<'a>),
}

pub type Block<'a> = Vec<Stmt<'a>>;

#[derive(Clone)]
pub struct Func<'a> {
    pub name: Spanned<&'a str>,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Spanned<Type<'a>>>,
    pub body: Block<'a>,
}

#[derive(Clone)]
pub struct Param<'a> {
    pub name: Spanned<&'a str>,
    pub r#type: Spanned<Type<'a>>,
}

#[derive(Clone, Copy)]
pub enum Type<'a> {
    Nothing,
    Int,
    Bool,
    String,
    Named(&'a str),
}

impl std::fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Type::Nothing => "nothing",
            Type::Int => "int",
            Type::Bool => "bool",
            Type::String => "string",
            Type::Named(name) => name,
        })
    }
}

#[derive(Clone)]
pub struct VarDecl<'a> {
    pub name: Spanned<&'a str>,
    pub value: Spanned<Expr<'a>>,
}

#[derive(Clone)]
pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    Var(&'a str),
    Int(&'a str),
    String(&'a str),
}

#[derive(Clone)]
pub struct Binary<'a> {
    pub op: Spanned<BinaryOp>,
    pub left: Box<Spanned<Expr<'a>>>,
    pub right: Box<Spanned<Expr<'a>>>,
}

#[derive(Clone)]
pub struct Unary<'a> {
    pub op: Spanned<UnaryOp>,
    pub expr: Box<Spanned<Expr<'a>>>,
}

#[derive(Clone)]
pub struct Call<'a> {
    pub name: Spanned<&'a str>,
    pub args: Vec<Spanned<Expr<'a>>>,
}

#[derive(Clone)]
pub struct Assign<'a> {
    pub op: Spanned<AssignOp>,
    pub target: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

fn map_to_vec<T, U>(slice: &[T], f: impl Fn(&T) -> U) -> Vec<U> {
    slice.iter().map(f).collect()
}

fn map_type<'a>(r#type: &Spanned<parser::Type<'a>>) -> Spanned<Type<'a>> {
    (
        match r#type.0 {
            parser::Type::Int => Type::Int,
            parser::Type::Bool => Type::Bool,
            parser::Type::String => Type::String,
            parser::Type::Named(name) => Type::Named(name),
        },
        r#type.1,
    )
}

fn map_expr<'a>((expr, span): &Spanned<parser::Expr<'a>>) -> Spanned<Expr<'a>> {
    (
        match expr {
            parser::Expr::Binary(left, op, right) => Expr::Binary(Binary {
                op: *op,
                left: Box::new(map_expr(left)),
                right: Box::new(map_expr(right)),
            }),
            parser::Expr::Unary(op, expr) => Expr::Unary(Unary {
                op: *op,
                expr: Box::new(map_expr(expr)),
            }),
            parser::Expr::Call(name, (args, _)) => Expr::Call(Call {
                name: *name,
                args: map_to_vec(args, map_expr),
            }),
            parser::Expr::Var(name) => Expr::Var(name),
            parser::Expr::Int(int) => Expr::Int(int),
            parser::Expr::String(string) => Expr::String(string),
        },
        *span,
    )
}

fn map_stmt<'a>(stmt: &parser::Stmt<'a>) -> Stmt<'a> {
    match stmt {
        parser::Stmt::Func(name, params, return_type, body) => Stmt::Func(Func {
            name: *name,
            params: map_to_vec(params, |parser::Param(name, r#type)| Param {
                name: *name,
                r#type: map_type(r#type),
            }),
            return_type: return_type.as_ref().map(map_type),
            body: map_to_vec(body, map_stmt),
        }),
        parser::Stmt::VarDecl(name, value) => Stmt::VarDecl(VarDecl {
            name: *name,
            value: map_expr(value),
        }),
        parser::Stmt::Expr(expr) => Stmt::Expr(map_expr(expr)),
        parser::Stmt::Assign(target, op, value) => Stmt::Assign(Assign {
            target: map_expr(target),
            op: *op,
            value: map_expr(value),
        }),
    }
}

pub fn ast_from_parse_tree<'a>(parse_tree: &ParseTree<'a>) -> Ast<'a> {
    Ast {
        filename: parse_tree.filename,
        stmts: map_to_vec(&parse_tree.stmts, map_stmt),
    }
}
