use chumsky::prelude::*;

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token<'a> {
    // Keywords
    Func,
    Var,
    If,
    Else,
    For,
    While,
    Loop,
    Break,
    Continue,
    Return,
    In,
    Import,
    // Tokens with stored data
    Ident(&'a str),
    Int(&'a str),
    Bool(bool),
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
            If => f.write_str("if"),
            Else => f.write_str("else"),
            For => f.write_str("for"),
            While => f.write_str("while"),
            Loop => f.write_str("loop"),
            Break => f.write_str("break"),
            Continue => f.write_str("continue"),
            Return => f.write_str("return"),
            In => f.write_str("in"),
            Import => f.write_str("import"),
            Ident(ident) => f.write_str(ident),
            Bool(bool) => write!(f, "{bool}"),
            Int(int) => f.write_str(int),
            String(string) => write!(f, "{string:?}"),
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
    let keyword = choice((
        text::ascii::keyword("func").to(Func),
        text::ascii::keyword("var").to(Var),
        text::ascii::keyword("if").to(If),
        text::ascii::keyword("else").to(Else),
        text::ascii::keyword("for").to(For),
        text::ascii::keyword("while").to(While),
        text::ascii::keyword("loop").to(Loop),
        text::ascii::keyword("break").to(Break),
        text::ascii::keyword("continue").to(Continue),
        text::ascii::keyword("return").to(Return),
        text::ascii::keyword("in").to(In),
        text::ascii::keyword("import").to(Import),
        text::ascii::keyword("true").to(Bool(true)),
        text::ascii::keyword("false").to(Bool(false)),
    ));
    let symbol = choice((
        just('{').to(LBrace),
        just('}').to(RBrace),
        just('(').to(LParen),
        just(')').to(RParen),
        just('[').to(LBracket),
        just(']').to(RBracket),
        just('.').to(Dot),
        just(',').to(Comma),
        just("->").to(Arrow),
        just(';').to(Semi),
        just('@').to(At),
        just('#').to(Hashtag),
        just('$').to(Dollar),
    ));
    choice((
        keyword,
        text::ident().map(Ident),
        text::int(10).map(Int),
        string.map(String),
        operator,
        symbol,
    ))
    .map_with(|token, e| (token, e.span()))
    .padded()
    .repeated()
    .collect()
}
