use chumsky::{input::ValueInput, prelude::*};

// TODO: error recovery
// TODO: tuples
// TODO: list/tuple pattern matching in variable declarations

use crate::{Span, Spanned, util::write_comma_separated};

pub type ParseError<'tokens, 'src> = Rich<'tokens, Token<'src>, Span>;

type ParseErrorExtra<'tokens, 'src> = extra::Err<Rich<'tokens, Token<'src>, Span>>;

pub struct ParseTree<'a> {
    pub filename: &'a str,
    pub stmts: Vec<Stmt<'a>>,
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

pub enum Stmt<'a> {
    Func(
        Spanned<&'a str>,
        Vec<Param<'a>>,
        Option<Spanned<Type<'a>>>,
        Block<'a>,
    ),
    VarDecl(Spanned<&'a str>, Spanned<Expr<'a>>),
    Expr(Spanned<Expr<'a>>),
    Assign(Spanned<Expr<'a>>, Spanned<AssignOp>, Spanned<Expr<'a>>),
    If(
        Spanned<Expr<'a>>,
        Block<'a>,
        Vec<(Spanned<Expr<'a>>, Block<'a>)>,
        Option<Block<'a>>,
    ),
    For(Spanned<&'a str>, Spanned<Expr<'a>>, Block<'a>),
    While(Spanned<Expr<'a>>, Block<'a>),
    Loop(Block<'a>),
    Break(Span),
    Continue(Span),
    Return(Spanned<Option<Spanned<Expr<'a>>>>),
}

pub type Block<'a> = Vec<Stmt<'a>>;

pub struct Param<'a>(pub Spanned<&'a str>, pub Spanned<Type<'a>>);

#[derive(Clone)]
pub struct Type<'a> {
    pub name: &'a str,
    pub generics: Vec<Spanned<Self>>,
}

impl std::fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { name, generics } = self;
        if !generics.is_empty() {
            write!(f, "{name}<")?;
            write_comma_separated(f, generics.iter().map(|(generic, _)| generic))?;
            return f.write_str(">");
        }
        f.write_str(name)
    }
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
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

pub enum Expr<'src> {
    Binary(Box<Spanned<Self>>, Spanned<BinaryOp>, Box<Spanned<Self>>),
    Unary(Spanned<UnaryOp>, Box<Spanned<Self>>),
    Call(Spanned<&'src str>, Spanned<Vec<Spanned<Self>>>),
    EmptyList(Spanned<Type<'src>>),
    List(Vec<Spanned<Self>>),
    EmptyMap(Spanned<Type<'src>>, Spanned<Type<'src>>),
    Map(Vec<(Spanned<Self>, Spanned<Self>)>),
    Index(Box<Spanned<Self>>, Box<Spanned<Self>>),
    Var(&'src str),
    Int(&'src str),
    Bool(bool),
    String(&'src str),
    // TODO: Char(char)
    Range(Box<Spanned<Self>>, Box<Spanned<Self>>),
}

use crate::Token;

fn with_span<'src, O, I, E>(o: O, e: &mut chumsky::input::MapExtra<'src, '_, I, E>) -> (O, I::Span)
where
    I: Input<'src>,
    E: extra::ParserExtra<'src, I>,
{
    (o, e.span())
}

pub fn create_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Vec<Stmt<'src>>, ParseErrorExtra<'tokens, 'src>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    let stmt = stmt_parser();
    stmt.repeated().collect()
}

fn stmt_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Stmt<'src>, ParseErrorExtra<'tokens, 'src>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    let ident = ident_parser();
    let r#type = type_parser();
    let param = ident
        .then_ignore(just(Token::Colon))
        .then(r#type.clone())
        .map(|(name, r#type)| Param(name, r#type));
    recursive(|stmt| {
        let block = stmt
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace));
        // TODO: generic functions
        let func = just(Token::Func)
            .ignore_then(ident)
            .then(
                param
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(just(Token::Arrow).ignore_then(r#type).or_not())
            .then(block.clone())
            .map(|(((name, params), return_type), body)| {
                Stmt::Func(name, params, return_type, body)
            });
        let expr = expr_parser();
        let var_decl = just(Token::Var)
            .ignore_then(ident)
            .then_ignore(just(Token::Op("=")))
            .then(expr.clone())
            .map(|(name, value)| Stmt::VarDecl(name, value));
        let assign = expr
            .clone()
            .then(
                choice((
                    just(Token::Op("=")).to(AssignOp::Assign),
                    just(Token::Op("+=")).to(AssignOp::Add),
                    just(Token::Op("-=")).to(AssignOp::Sub),
                    just(Token::Op("*=")).to(AssignOp::Mul),
                    just(Token::Op("/=")).to(AssignOp::Div),
                    just(Token::Op("%=")).to(AssignOp::Mod),
                ))
                .map_with(with_span),
            )
            .then(expr.clone())
            .map(|((target, op), value)| Stmt::Assign(target, op, value));
        let if_head = just(Token::If)
            .ignore_then(expr.clone())
            .then(block.clone());
        let if_stmt = if_head
            .clone() // if
            .then(
                just(Token::Else)
                    .ignore_then(if_head)
                    .repeated()
                    .collect::<Vec<_>>(),
            ) // else if
            .then(just(Token::Else).ignore_then(block.clone()).or_not()) // else
            .map(|(((cond, then_block), else_if_blocks), else_block)| {
                Stmt::If(cond, then_block, else_if_blocks, else_block)
            });
        let for_stmt = just(Token::For)
            .ignore_then(ident)
            .then_ignore(just(Token::In))
            .then(
                expr.clone()
                    .then(just(Token::DotDot).ignore_then(expr.clone()).or_not())
                    .map_with(with_span)
                    .map(|((expr, range_end), range_span)| match range_end {
                        Some(end) => (Expr::Range(Box::new(expr), Box::new(end)), range_span),
                        None => expr,
                    }),
            )
            .then(block.clone())
            .map(|((var, iter), block)| Stmt::For(var, iter, block));
        let while_stmt = just(Token::While)
            .ignore_then(expr.clone())
            .then(block.clone())
            .map(|(cond, block)| Stmt::While(cond, block));
        let loop_stmt = just(Token::Loop).ignore_then(block).map(Stmt::Loop);
        choice((
            func,
            var_decl,
            assign,
            if_stmt,
            for_stmt,
            while_stmt,
            loop_stmt,
            just(Token::Break).map_with(|_, e| Stmt::Break(e.span())),
            just(Token::Continue).map_with(|_, e| Stmt::Continue(e.span())),
            just(Token::Return)
                .ignore_then(expr.clone().or_not().map_with(with_span))
                .map(|expr| Stmt::Return(expr)),
            expr.map(Stmt::Expr),
        ))
        .then_ignore(just(Token::Semi).or_not())
    })
}

fn type_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<Type<'src>>, ParseErrorExtra<'tokens, 'src>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    recursive(|r#type| {
        select! {
            Token::Ident(ident) => ident
        }
        .then(
            r#type
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::Op("<")), just(Token::Op(">")))
                .or_not(),
        )
        .map(|(name, generics)| Type {
            name,
            generics: generics.unwrap_or(Vec::new()),
        })
        .map_with(with_span)
    })
}

fn ident_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<&'src str>, ParseErrorExtra<'tokens, 'src>> + Copy
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    select! { Token::Ident(ident) => ident }.map_with(with_span)
}

fn expr_parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, Spanned<Expr<'src>>, ParseErrorExtra<'tokens, 'src>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    let ident = ident_parser();
    let var = select! { Token::Ident(ident) => Expr::Var(ident) }.map_with(with_span);
    let int = select! { Token::Int(int) => Expr::Int(int) }.map_with(with_span);
    let bool = select! { Token::Bool(bool) => Expr::Bool(bool) }.map_with(with_span);
    let string = select! { Token::String(string) => Expr::String(string) }.map_with(with_span);
    let r#type = type_parser();

    recursive(|expr| {
        // Function calls have very high precedence so we prioritise them
        let call = ident
            .then(
                expr.clone()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen))
                    .map_with(with_span),
            )
            .map(|(name, args)| Expr::Call(name, args))
            .map_with(with_span)
            .boxed();
        // TODO: make specifying more than one type instead of [<type>] report a nice error message
        let list = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(Expr::List)
            .or(r#type
                .clone()
                .delimited_by(
                    // Have to do this instead of nested delimited_by (which would be cleaner)
                    // because chumsky doesn't support that.
                    just(Token::LBracket).then(just(Token::Op("<"))),
                    just(Token::Op(">")).then(just(Token::RBracket)),
                )
                .map(Expr::EmptyList))
            .map_with(with_span);
        // TODO: make specifying only one type instead of {<key, value>} report a nice error message
        let map = expr
            .clone()
            .then_ignore(just(Token::Colon))
            .then(expr.clone())
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(Expr::Map)
            .or(r#type
                .clone()
                .then_ignore(just(Token::Comma))
                .then(r#type.clone())
                .delimited_by(
                    // Have to do this instead of nested delimited_by (which would be cleaner)
                    // because chumsky doesn't support that.
                    just(Token::LBrace).then(just(Token::Op("<"))),
                    just(Token::Op(">")).then(just(Token::RBrace)),
                )
                .map(|(key_type, value_type)| Expr::EmptyMap(key_type, value_type)))
            .map_with(with_span);

        // Unambiguous expressions
        let atom = choice((
            call,
            list,
            map,
            var,
            int,
            bool,
            string,
            expr.clone()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        ))
        .boxed();

        let index = atom
            .clone()
            .then(
                expr.clone()
                    .delimited_by(just(Token::LBracket), just(Token::LBracket)),
            )
            .map(|(expr, index)| Expr::Index(Box::new(expr), Box::new(index)))
            .map_with(with_span);

        // Product ops (multiply and divide) have equal precedence
        let op = choice((
            just(Token::Op("*")).to(BinaryOp::Mul),
            just(Token::Op("/")).to(BinaryOp::Div),
            just(Token::Op("%")).to(BinaryOp::Mod),
        ))
        .map_with(with_span);
        let product = index
            .clone()
            .foldl_with(op.then(index.clone()).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            });

        // Sum ops (add and subtract) have equal precedence
        let op = just(Token::Op("+"))
            .to(BinaryOp::Add)
            .or(just(Token::Op("-")).to(BinaryOp::Sub))
            .map_with(with_span);
        let sum = product
            .clone()
            .foldl_with(op.then(product).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        // Comparison ops (<, <=, >, >=) have equal precedence
        let op = choice((
            just(Token::Op("<")).to(BinaryOp::Lt),
            just(Token::Op("<=")).to(BinaryOp::Leq),
            just(Token::Op(">")).to(BinaryOp::Gt),
            just(Token::Op(">=")).to(BinaryOp::Geq),
        ))
        .map_with(with_span);
        let ordering = sum
            .clone()
            .foldl_with(op.then(sum).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            });

        // Comparison ops (equal, not-equal) have equal precedence
        let op = just(Token::Op("=="))
            .to(BinaryOp::Eq)
            .or(just(Token::Op("!=")).to(BinaryOp::NotEq))
            .map_with(with_span);
        let equal = ordering
            .clone()
            .foldl_with(op.then(ordering).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            });

        let op = just(Token::Op("and")).to(BinaryOp::And).map_with(with_span);
        let and = equal
            .clone()
            .foldl_with(op.then(equal).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            })
            .boxed();

        let op = just(Token::Op("or")).to(BinaryOp::Or).map_with(with_span);
        let or = and
            .clone()
            .foldl_with(op.then(and).repeated(), |a, (op, b), e| {
                (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
            });

        let binary = or;
        let unary = just(Token::Op("-"))
            .to(UnaryOp::Neg)
            .or(just(Token::Op("not")).to(UnaryOp::Not))
            .map_with(with_span)
            .then(index.clone())
            .map(|(op, expr)| Expr::Unary(op, Box::new(expr)))
            .map_with(with_span);
        choice((binary, unary, index, atom))
    })
    .boxed()
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
