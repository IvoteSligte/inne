
pub type Ast<'a> = Vec<Stmt<'a>>;

pub enum Stmt<'a> {
    Func(Func<'a>),
    Expr(Expr<'a>),
}

pub struct Func<'a> {
    pub name: &'a str,
    pub params: Vec<Param<'a>>,
    pub return_type: Option<Type<'a>>,
    pub body: Vec<Stmt<'a>>,
}

pub struct Param<'a> {
    pub name: &'a str,
    pub r#type: Type<'a>,
}

pub enum Type<'a> {
    Int,
    String,
    Named(&'a str),
}

pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Var(&'a str),
    Int(&'a str),
    String(&'a str),
}

pub struct Binary<'a> {
    pub op: &'a str,
    pub left: Box<Expr<'a>>,
    pub right: Box<Expr<'a>>,
}

pub struct Unary<'a> {
    pub op: &'a str,
    pub expr: Box<Expr<'a>>,
}
