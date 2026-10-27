use crate::{Span, Spanned, util::write_comma_separated};

#[derive(Debug)]
pub struct Ast<'a> {
    pub filename: &'a str,
    pub stmts: Vec<Stmt<'a>>,
}

pub type Name<'a> = Spanned<&'a str>;

#[derive(Debug)]
pub enum Stmt<'a> {
    Func(Func<'a>),
    Struct(Struct<'a>),
    Enum(Enum<'a>),
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

#[derive(Debug, Clone)]
pub struct Type<'a> {
    /// Uses `%tuple` to indicate tuple types.
    pub name: Name<'a>,
    pub generics: Vec<Spanned<Self>>,
}

#[derive(Debug)]
pub struct Func<'a> {
    pub name: Name<'a>,
    pub generics: Vec<Name<'a>>,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Spanned<Type<'a>>>,
    pub body: Block<'a>,
}

#[derive(Debug)]
pub struct Param<'a> {
    pub name: Name<'a>,
    pub r#type: Spanned<Type<'a>>,
}

#[derive(Debug)]
pub struct Struct<'a> {
    pub name: Name<'a>,
    pub fields: Vec<Field<'a>>,
}

#[derive(Debug)]
pub struct Field<'a> {
    pub name: Name<'a>,
    pub r#type: Spanned<Type<'a>>,
}

#[derive(Debug)]
pub struct Enum<'a> {
    pub name: Name<'a>,
    pub variants: Vec<Variant<'a>>,
}

#[derive(Debug)]
pub struct Variant<'a> {
    pub name: Name<'a>,
    pub payload: Option<Spanned<Type<'a>>>,
}

#[derive(Debug)]
pub struct VarDecl<'a> {
    pub name: Name<'a>,
    pub value: Spanned<Expr<'a>>,
}

#[derive(Debug)]
pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    EmptyList(EmptyList<'a>),
    List(List<'a>),
    EmptyMap(EmptyMap<'a>),
    Map(Map<'a>),
    StructLiteral(StructLiteral<'a>),
    VariantLiteral(VariantLiteral<'a>),
    Member(Member<'a>),
    Index(Index<'a>),
    Range(Range<'a>),
    Var(&'a str),
    Int(&'a str),
    Bool(bool),
    Char(&'a str),
    String(&'a str),
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Unary<'a> {
    pub op: Spanned<UnaryOp>,
    pub expr: Box<Spanned<Expr<'a>>>,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug)]
pub struct Call<'a> {
    pub name: Name<'a>,
    pub args: Vec<Spanned<Expr<'a>>>,
}

#[derive(Debug)]
pub struct EmptyList<'a> {
    pub element_type: Spanned<Type<'a>>,
}

#[derive(Debug)]
pub struct List<'a> {
    pub elements: Vec<Spanned<Expr<'a>>>,
}

#[derive(Debug)]
pub struct EmptyMap<'a> {
    pub key_type: Spanned<Type<'a>>,
    pub value_type: Spanned<Type<'a>>,
}

#[derive(Debug)]
pub struct Map<'a> {
    pub pairs: Vec<Pair<'a>>,
}

#[derive(Debug)]
pub struct Pair<'a> {
    pub key: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

#[derive(Debug)]
pub struct StructLiteral<'a> {
    pub name: Name<'a>,
    pub fields: Vec<FieldLiteral<'a>>,
}

#[derive(Debug)]
pub struct FieldLiteral<'a> {
    pub name: Name<'a>,
    pub value: Spanned<Expr<'a>>,
}

#[derive(Debug)]
pub struct VariantLiteral<'a> {
    pub enum_name: Name<'a>,
    pub name: Name<'a>,
    pub payload: Option<Box<Spanned<Expr<'a>>>>,
}

#[derive(Debug)]
pub struct Member<'a> {
    pub expr: Box<Spanned<Expr<'a>>>,
    pub member: Name<'a>,
}

#[derive(Debug)]
pub struct Index<'a> {
    pub expr: Box<Spanned<Expr<'a>>>,
    pub index: Box<Spanned<Expr<'a>>>,
}

#[derive(Debug)]
pub struct Range<'a> {
    pub start: Box<Spanned<Expr<'a>>>,
    pub end: Box<Spanned<Expr<'a>>>,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct If<'a> {
    pub cond: Spanned<Expr<'a>>,
    pub then: Block<'a>,
    pub else_ifs: Vec<(Spanned<Expr<'a>>, Block<'a>)>,
    pub r#else: Option<Block<'a>>,
}

#[derive(Debug)]
pub struct For<'a> {
    pub var: Spanned<&'a str>,
    pub iter: Spanned<Expr<'a>>,
    pub body: Block<'a>,
}

#[derive(Debug)]
pub struct While<'a> {
    pub cond: Spanned<Expr<'a>>,
    pub body: Block<'a>,
}

#[derive(Debug)]
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
