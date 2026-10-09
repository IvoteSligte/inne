use chumsky::{input::ValueInput, prelude::*};

mod diagnosis;

use crate::{Span, Spanned};

pub type ParseTree<'a> = Vec<Spanned<Stmt<'a>>>;

#[derive(Debug, Clone, Copy)]
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
    Expr(Expr<'a>),
    Assign(Spanned<Expr<'a>>, AssignOp, Spanned<Expr<'a>>),
}

pub type Block<'a> = Spanned<Vec<Spanned<Stmt<'a>>>>;

pub struct Param<'a>(pub Spanned<&'a str>, pub Spanned<Type<'a>>);

pub enum Type<'a> {
    Int,
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
    Binary(Box<Spanned<Self>>, BinaryOp, Box<Spanned<Self>>),
    Unary(UnaryOp, Box<Spanned<Self>>),
    Call(Box<Spanned<Self>>, Spanned<Vec<Spanned<Self>>>),
    Var(&'a str),
    Int(&'a str),
    String(&'a str),
}

use crate::Token;

pub fn parser<'tokens, 'src: 'tokens, I>()
-> impl Parser<'tokens, I, ParseTree<'src>, extra::Err<Rich<'tokens, Token<'src>, Span>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = Span>,
{
    let ident = select! { Token::Ident(ident) => ident}.map_with(|ident, e| (ident, e.span()));
    let r#type = select! {
        Token::Ident(ident) => match ident {
            "int" => Type::Int,
            "string" => Type::String,
            _ => Type::Named(ident),
        }
    }
    .map_with(|r#type, e| (r#type, e.span()));
    let param = ident.then(r#type).map(|(name, r#type)| Param(name, r#type));
    let stmt = recursive(|stmt| {
        let block = stmt
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|block, e| (block, e.span()));
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
            .then(block)
            .map(|(((name, params), return_type), body)| {
                Stmt::Func(name, params, return_type, body)
            })
            .map_with(|func, e| (func, e.span()));

        let var =
            select! { Token::Ident(ident) => Expr::Var(ident) }.map_with(|var, e| (var, e.span()));
        let int = select! { Token::Int(int) => Expr::Int(int) }.map_with(|var, e| (var, e.span()));
        let string = select! { Token::String(string) => Expr::String(string) }
            .map_with(|var, e| (var, e.span()));

        let expr = recursive(|expr| {
            // A list of expressions
            let items = expr
                .clone()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>();

            let atom = var.or(int).or(string);

            // Function calls have very high precedence so we prioritise them
            let call = atom.foldl_with(
                items
                    .delimited_by(just(Token::LParen), just(Token::RParen))
                    .map_with(|args, e| (args, e.span()))
                    .repeated(),
                |f, args, e| (Expr::Call(Box::new(f), args), e.span()),
            );

            // Product ops (multiply and divide) have equal precedence
            let op = just(Token::Op("*"))
                .to(BinaryOp::Mul)
                .or(just(Token::Op("/")).to(BinaryOp::Div));
            let product = call
                .clone()
                .foldl_with(op.then(call.clone()).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            // Sum ops (add and subtract) have equal precedence
            let op = just(Token::Op("+"))
                .to(BinaryOp::Add)
                .or(just(Token::Op("-")).to(BinaryOp::Sub));
            let sum = product
                .clone()
                .foldl_with(op.then(product).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            // Comparison ops (equal, not-equal) have equal precedence
            let op = just(Token::Op("=="))
                .to(BinaryOp::Eq)
                .or(just(Token::Op("!=")).to(BinaryOp::NotEq));
            let compare = sum
                .clone()
                .foldl_with(op.then(sum).repeated(), |a, (op, b), e| {
                    (Expr::Binary(Box::new(a), op, Box::new(b)), e.span())
                });

            let binary = compare;
            let unary = just(Token::Op("-"))
                .to(UnaryOp::Neg)
                .or(just(Token::Op("not")).to(UnaryOp::Not))
                .then(call.clone())
                .map(|(op, expr)| Expr::Unary(op, Box::new(expr)))
                .map_with(|unary, e| (unary, e.span()));
            choice((binary, unary, call, var, int, string))
        });
        let assign = expr
            .clone()
            .then(choice((
                just(Token::Op("=")).to(AssignOp::Assign),
                just(Token::Op("+=")).to(AssignOp::Add),
                just(Token::Op("-=")).to(AssignOp::Sub),
                just(Token::Op("*=")).to(AssignOp::Mul),
                just(Token::Op("/=")).to(AssignOp::Div),
            )))
            .then(expr.clone())
            .map(|((target, op), value)| Stmt::Assign(target, op, value))
            .map_with(|assign, e| (assign, e.span()));
        choice((
            func,
            expr.map(|(expr, span)| (Stmt::Expr(expr), span)),
            assign,
        ))
        .then_ignore(just(Token::Semi).or_not())
    });
    stmt.repeated().collect()
}
