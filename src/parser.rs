use chumsky::{input::ValueInput, prelude::*};

mod diagnosis;

// TODO: error recovery

use crate::{Span, Spanned};

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

pub enum Type<'a> {
    Int,
    Bool,
    String,
    Named(&'a str),
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

pub enum Expr<'a> {
    Binary(Box<Spanned<Self>>, Spanned<BinaryOp>, Box<Spanned<Self>>),
    Unary(Spanned<UnaryOp>, Box<Spanned<Self>>),
    Call(Spanned<&'a str>, Spanned<Vec<Spanned<Self>>>),
    Var(&'a str),
    Int(&'a str),
    Bool(bool),
    String(&'a str),
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

pub fn parser<'tokens, 'src: 'tokens, I>(
    filename: &'src str,
) -> impl Parser<'tokens, I, ParseTree<'src>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    let ident = select! { Token::Ident(ident) => ident }.map_with(with_span);
    let r#type = select! {
        Token::Ident(ident) => match ident {
            "int" => Type::Int,
            "bool" => Type::Bool,
            "string" => Type::String,
            _ => Type::Named(ident),
        }
    }
    .map_with(with_span);
    let param = ident.then(r#type).map(|(name, r#type)| Param(name, r#type));
    let stmt = recursive(|stmt| {
        let block = stmt
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace));
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

        let var = select! { Token::Ident(ident) => Expr::Var(ident) }.map_with(with_span);
        let int = select! { Token::Int(int) => Expr::Int(int) }.map_with(with_span);
        let bool = select! { Token::Bool(bool) => Expr::Bool(bool) }.map_with(with_span);
        let string = select! { Token::String(string) => Expr::String(string) }.map_with(with_span);

        let expr = recursive(|expr| {
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
                .map_with(with_span);

            // Unambiguous expressions
            let atom = choice((call, var, int, bool, string));

            // Product ops (multiply and divide) have equal precedence
            let op = just(Token::Op("*"))
                .to(BinaryOp::Mul)
                .or(just(Token::Op("/")).to(BinaryOp::Div))
                .map_with(with_span);
            let product = atom
                .clone()
                .foldl_with(op.then(atom.clone()).repeated(), |a, (op, b), e| {
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
                });

            // Comparison ops (equal, not-equal) have equal precedence
            let op = just(Token::Op("=="))
                .to(BinaryOp::Eq)
                .or(just(Token::Op("!=")).to(BinaryOp::NotEq))
                .map_with(with_span);
            let compare = sum
                .clone()
                .foldl_with(op.then(sum).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            let binary = compare;
            let unary = just(Token::Op("-"))
                .to(UnaryOp::Neg)
                .or(just(Token::Op("not")).to(UnaryOp::Not))
                .map_with(with_span)
                .then(atom.clone())
                .map(|(op, expr)| Expr::Unary(op, Box::new(expr)))
                .map_with(with_span);
            choice((binary, unary, atom))
        });
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
    });
    stmt.repeated()
        .collect()
        .map(|stmts| ParseTree { filename, stmts })
}
