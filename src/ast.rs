use crate::{ParseTree, Spanned, parser};

pub use crate::parser::{AssignOp, BinaryOp, UnaryOp};

pub type Ast<'a> = Vec<Stmt<'a>>;

pub enum Stmt<'a> {
    Func(Func<'a>),
    Expr(Expr<'a>),
    Assign(Assign<'a>),
}

pub type Block<'a> = Spanned<Vec<Spanned<Stmt<'a>>>>;

pub struct Func<'a> {
    pub name: Spanned<&'a str>,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Spanned<Type<'a>>>,
    pub body: Block<'a>,
}

pub struct Param<'a> {
    pub name: Spanned<&'a str>,
    pub r#type: Spanned<Type<'a>>,
}

pub enum Type<'a> {
    Int,
    String,
    Named(&'a str),
}

pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    Var(&'a str),
    Int(&'a str),
    String(&'a str),
}

pub struct Binary<'a> {
    pub op: BinaryOp,
    pub left: Box<Spanned<Expr<'a>>>,
    pub right: Box<Spanned<Expr<'a>>>,
}

pub struct Unary<'a> {
    pub op: UnaryOp,
    pub expr: Box<Spanned<Expr<'a>>>,
}

pub struct Call<'a> {
    pub func: Box<Spanned<Expr<'a>>>,
    pub args: Vec<Spanned<Expr<'a>>>,
}

pub struct Assign<'a> {
    pub op: AssignOp,
    pub target: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

fn map_to_vec<T, U>(slice: &[T], f: impl Fn(&T) -> U) -> Vec<U> {
    slice.iter().map(f).collect()
}

fn map_spanned_type<'a>(r#type: &Spanned<parser::Type<'a>>) -> Spanned<Type<'a>> {
    (
        match r#type.0 {
            parser::Type::Int => Type::Int,
            parser::Type::String => Type::String,
            parser::Type::Named(name) => Type::Named(name),
        },
        r#type.1,
    )
}

fn map_expr<'a>(expr: &parser::Expr<'a>) -> Expr<'a> {
    match expr {
        parser::Expr::Binary(left, op, right) => Expr::Binary(Binary {
            op: *op,
            left: Box::new(map_spanned_expr(left)),
            right: Box::new(map_spanned_expr(right)),
        }),
        parser::Expr::Unary(op, expr) => Expr::Unary(Unary {
            op: *op,
            expr: Box::new(map_spanned_expr(expr)),
        }),
        parser::Expr::Call(func, (args, _)) => Expr::Call(Call {
            func: Box::new(map_spanned_expr(func)),
            args: map_to_vec(args, map_spanned_expr),
        }),
        parser::Expr::Var(name) => Expr::Var(name),
        parser::Expr::Int(int) => Expr::Int(int),
        parser::Expr::String(string) => Expr::String(string),
    }
}

fn map_spanned_expr<'a>((expr, span): &Spanned<parser::Expr<'a>>) -> Spanned<Expr<'a>> {
    (map_expr(expr), *span)
}

fn map_stmt<'a>(stmt: &parser::Stmt<'a>) -> Stmt<'a> {
    match stmt {
        parser::Stmt::Func(name, params, return_type, body) => Stmt::Func(Func {
            name: *name,
            params: map_to_vec(params, |parser::Param(name, r#type)| Param {
                name: *name,
                r#type: map_spanned_type(r#type),
            }),
            return_type: return_type.as_ref().map(map_spanned_type),
            body: map_block(body),
        }),
        parser::Stmt::Expr(expr) => Stmt::Expr(map_expr(expr)),
        parser::Stmt::Assign(target, op, value) => Stmt::Assign(Assign {
            target: map_spanned_expr(target),
            op: *op,
            value: map_spanned_expr(value),
        }),
    }
}

fn map_spanned_stmt<'a>((stmt, span): &Spanned<parser::Stmt<'a>>) -> Spanned<Stmt<'a>> {
    (map_stmt(stmt), *span)
}

fn map_block<'a>((block, span): &parser::Block<'a>) -> Block<'a> {
    (map_to_vec(block, map_spanned_stmt), *span)
}

pub fn parse_tree_to_ast<'a>(parse_tree: &ParseTree<'a>) -> Ast<'a> {
    parse_tree.iter().map(|(stmt, _)| map_stmt(stmt)).collect()
}
