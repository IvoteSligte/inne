use chumsky::prelude::*;

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token<'a> {
    // Keywords
    Func,
    Var,
    // Tokens with stored data
    Ident(&'a str),
    Int(&'a str),
    String(&'a str),
    Op(&'a str),
    // Delimiters
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    // Punctuation
    Dot,
    Comma,
    Arrow,
    Semi,
    At,
    Hashtag,
    Dollar,
}

impl<'a> std::fmt::Display for Token<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Func => f.write_str("func"),
            Var => f.write_str("var"),
            Ident(ident) => f.write_str(ident),
            Int(int) => f.write_str(int),
            String(string) => write!(f, "\"{string}\""),
            Op(op) => f.write_str(op),
            LBrace => f.write_str("("),
            RBrace => f.write_str("]"),
            LParen => f.write_str("("),
            RParen => f.write_str(")"),
            LBracket => f.write_str("{"),
            RBracket => f.write_str("}"),
            Dot => f.write_str("."),
            Comma => f.write_str(","),
            Arrow => f.write_str("->"),
            Semi => f.write_str(";"),
            At => f.write_str("@"),
            Hashtag => f.write_str("#"),
            Dollar => f.write_str("$"),
        }
    }
}

use Token::*;

pub fn lexer<'a>()
-> impl Parser<'a, &'a str, Vec<Spanned<Token<'a>>>, extra::Err<Rich<'a, char, Span>>> {
    let escape = just('\\')
        .then(choice((
            just('\\'),
            just('/'),
            just('"'),
            just('b').to('\x08'),
            just('f').to('\x0C'),
            just('n').to('\n'),
            just('r').to('\r'),
            just('t').to('\t'),
            just('u').ignore_then(text::digits(16).exactly(4).to_slice().validate(
                |digits, e, emitter| {
                    char::from_u32(u32::from_str_radix(digits, 16).unwrap()).unwrap_or_else(|| {
                        emitter.emit(Rich::custom(e.span(), "invalid unicode character"));
                        '\u{FFFD}' // unicode replacement character
                    })
                },
            )),
        )))
        .ignored();

    let string = none_of("\\\"")
        .ignored()
        .or(escape)
        .repeated()
        .to_slice()
        .delimited_by(just('"'), just('"'));

    let operator = choice((
        just("&&"),
        just("||"),
        just("=="),
        just("!="),
        just("<="),
        just(">="),
        just("+="),
        just("-="),
        just("*="),
        just("/="),
        just("%="),
        just("="),
        just("%"),
        just("*"),
        just("/"),
        just("+"),
        just("-"),
        just("<"),
        just(">"),
    ))
    .map(Op);

    choice((
        text::ascii::keyword("func").map(|_| Func),
        text::ascii::keyword("var").map(|_| Var),
        text::ident().map(Ident),
        text::int(10).map(Int),
        string.map(String),
        just('.').map(|_| Dot),
        just(',').map(|_| Comma),
        just("->").map(|_| Arrow),
        just(';').map(|_| Semi),
        operator,
        just('@').map(|_| At),
        just('#').map(|_| Hashtag),
        just('$').map(|_| Dollar),
    ))
    .map_with(|token, e| (token, e.span()))
    .padded()
    .repeated()
    .collect()
}
