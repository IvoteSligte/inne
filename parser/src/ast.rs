use crate::{Span, Spanned, util::write_comma_separated};

pub struct Ast<'a> {
    pub filename: &'a str,
    pub stmts: Vec<Stmt<'a>>,
}

pub type Name<'a> = Spanned<&'a str>;

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

#[derive(Clone)]
pub struct Type<'a> {
    pub name: Name<'a>,
    pub generics: Vec<Spanned<Self>>,
}

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

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    Leq,
    Geq,
    And,
    Or,
    // TODO: make Range a binary operator
}

pub struct Unary<'a> {
    pub op: Spanned<UnaryOp>,
    pub expr: Box<Spanned<Expr<'a>>>,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

pub struct Call<'a> {
    pub name: Name<'a>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
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

impl std::fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { name, generics } = self;
        f.write_str(name.0)?;
        if !generics.is_empty() {
            f.write_str("<")?;
            write_comma_separated(f, generics.iter().map(|(generic, _)| generic))?;
            return f.write_str(">");
        }
        Ok(())
    }
}


impl std::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AssignOp::Assign => "=",
            AssignOp::Add => "+=",
            AssignOp::Sub => "-=",
            AssignOp::Mul => "*=",
            AssignOp::Div => "/=",
            AssignOp::Mod => "%=",
        })
    }
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Leq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Geq => ">=",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
        })
    }
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "not",
        })
    }
}
