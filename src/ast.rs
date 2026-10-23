use crate::{ParseTree, Span, Spanned};

pub use parser::{AssignOp, BinaryOp, Name, Type, UnaryOp};

pub struct Ast<'a> {
    pub filename: &'a str,
    pub stmts: Vec<Stmt<'a>>,
}

pub enum Stmt<'a> {
    Func(Func<'a>),
    Struct(Struct<'a>),
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
    pub name: Name<'a>,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Spanned<Type<'a>>>,
    pub body: Block<'a>,
}

pub struct Param<'a> {
    pub name: Name<'a>,
    pub r#type: Spanned<Type<'a>>,
}

pub struct Struct<'a> {
    pub name: Name<'a>,
    pub fields: Vec<Field<'a>>,
}

pub struct Field<'a> {
    pub name: Name<'a>,
    pub r#type: Spanned<Type<'a>>,
}

pub struct VarDecl<'a> {
    pub name: Name<'a>,
    pub value: Spanned<Expr<'a>>,
}

pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    EmptyList(EmptyList<'a>),
    List(List<'a>),
    EmptyMap(EmptyMap<'a>),
    Map(Map<'a>),
    StructLiteral(StructLiteral<'a>),
    Index(Index<'a>),
    Range(Range<'a>),
    Var(&'a str),
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
    pub args: Vec<Spanned<Expr<'a>>>,
}

pub struct EmptyList<'a> {
    pub element_type: Spanned<Type<'a>>,
}

pub struct List<'a> {
    pub elements: Vec<Spanned<Expr<'a>>>,
}

pub struct EmptyMap<'a> {
    pub key_type: Spanned<Type<'a>>,
    pub value_type: Spanned<Type<'a>>,
}

pub struct Map<'a> {
    pub pairs: Vec<Pair<'a>>,
}

pub struct Pair<'a> {
    pub key: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

pub struct StructLiteral<'a> {
    pub name: Name<'a>,
    pub fields: Vec<FieldLiteral<'a>>,
}

pub struct FieldLiteral<'a> {
    pub name: Name<'a>,
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
    pub cond: Spanned<Expr<'a>>,
    pub then: Block<'a>,
    pub else_ifs: Vec<(Spanned<Expr<'a>>, Block<'a>)>,
    pub r#else: Option<Block<'a>>,
}

pub struct For<'a> {
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

fn map_to_vec<T, U>(slice: &[T], f: impl Fn(&T) -> U) -> Vec<U> {
    slice.iter().map(f).collect()
}

fn map_block<'a>(block: &[parser::Stmt<'a>]) -> Vec<Stmt<'a>> {
    map_to_vec(block, map_stmt)
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
            parser::Expr::EmptyList(element_type) => Expr::EmptyList(EmptyList {
                element_type: element_type.clone(),
            }),
            parser::Expr::List(elements) => Expr::List(List {
                elements: map_to_vec(elements, map_expr),
            }),
            parser::Expr::EmptyMap(key_type, value_type) => Expr::EmptyMap(EmptyMap {
                key_type: key_type.clone(),
                value_type: value_type.clone(),
            }),
            parser::Expr::Map(pairs) => Expr::Map(Map {
                pairs: map_to_vec(pairs, |(key, value)| Pair {
                    key: map_expr(key),
                    value: map_expr(value),
                }),
            }),
            parser::Expr::StructLiteral(name, fields) => Expr::StructLiteral(StructLiteral {
                name: *name,
                fields: map_to_vec(fields, |(name, value)| FieldLiteral {
                    name: *name,
                    value: map_expr(value),
                }),
            }),
            parser::Expr::Index(expr, index) => Expr::Index(Index {
                expr: Box::new(map_expr(expr)),
                index: Box::new(map_expr(index)),
            }),
            parser::Expr::Var(name) => Expr::Var(name),
            parser::Expr::Int(int) => Expr::Int(int),
            parser::Expr::Bool(bool) => Expr::Bool(*bool),
            parser::Expr::String(string) => Expr::String(string),
            parser::Expr::Range(start, end) => Expr::Range(Range {
                start: Box::new(map_expr(start)),
                end: Box::new(map_expr(end)),
            }),
        },
        *span,
    )
}

fn map_stmt<'a>(stmt: &parser::Stmt<'a>) -> Stmt<'a> {
    match stmt {
        parser::Stmt::Func(name, params, return_type, body) => Stmt::Func(Func {
            name: *name,
            params: map_to_vec(params, |(name, r#type)| Param {
                name: *name,
                r#type: r#type.clone(),
            }),
            return_type: return_type.clone(),
            body: map_block(body),
        }),
        parser::Stmt::Struct(name, fields) => Stmt::Struct(Struct {
            name: *name,
            fields: map_to_vec(fields, |(name, r#type)| Field {
                name: *name,
                r#type: r#type.clone(),
            }),
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
        parser::Stmt::If(cond, then, else_ifs, r#else) => Stmt::If(If {
            cond: map_expr(cond),
            then: map_block(then),
            else_ifs: map_to_vec(else_ifs, |(cond, then)| (map_expr(cond), map_block(then))),
            r#else: r#else.as_deref().map(map_block),
        }),
        parser::Stmt::For(var, iter, body) => Stmt::For(For {
            var: *var,
            iter: map_expr(iter),
            body: map_block(body),
        }),
        parser::Stmt::While(cond, body) => Stmt::While(While {
            cond: map_expr(cond),
            body: map_block(body),
        }),
        parser::Stmt::Loop(body) => Stmt::Loop(Loop {
            body: map_block(body),
        }),
        parser::Stmt::Break(span) => Stmt::Break(*span),
        parser::Stmt::Continue(span) => Stmt::Continue(*span),
        parser::Stmt::Return((value, span)) => Stmt::Return((value.as_ref().map(map_expr), *span)),
    }
}

pub fn ast_from_parse_tree<'a>(parse_tree: &ParseTree<'a>) -> Ast<'a> {
    Ast {
        filename: parse_tree.filename,
        stmts: map_to_vec(&parse_tree.stmts, map_stmt),
    }
}
