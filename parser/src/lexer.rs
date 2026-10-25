use chumsky::prelude::*;

pub type Span = SimpleSpan;
pub type Spanned<T> = (T, Span);
pub type LexError<'src> = Rich<'src, char, Span>;

type LexErrorExtra<'src> = extra::Err<LexError<'src>>;

// TODO: comments

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token<'src> {
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
    Use,
    Struct,
    Enum,
    Ref,
    Copy,
    ShallowCopy,
    // Tokens with stored data
    Ident(&'src str),
    Int(&'src str),
    Bool(bool),
    Char(&'src str),
    String(&'src str),
    // TODO: change Op("<") and Op(">") to explicit Delimiter tokens
    Op(&'src str),
    Comment(&'src str),
    // Delimiters
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    // Punctuation
    Dot,
    DotDot,
    Comma,
    Arrow,
    Semi,
    Colon,
    ColonColon,
    At,
    Hashtag,
    Dollar,
}

impl Token<'_> {
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            Func | Var
                | If
                | Else
                | For
                | While
                | Loop
                | Break
                | Continue
                | Return
                | In
                | Import
                | Use
                | Struct
                | Enum
                | Ref
                | Copy
                | ShallowCopy
        )
    }
}

impl std::fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Func => "func",
            Var => "var",
            If => "if",
            Else => "else",
            For => "for",
            While => "while",
            Loop => "loop",
            Break => "break",
            Continue => "continue",
            Return => "return",
            In => "in",
            Import => "import",
            Use => "use",
            Struct => "struct",
            Enum => "enum",
            Ref => "ref",
            Copy => "copy",
            ShallowCopy => "shallow_copy",
            Ident(ident) => ident,
            Bool(bool) => return write!(f, "{bool}"),
            Int(int) => int,
            // NOTE: does this escape backslashes? since the input may contain \n, we should not escape backslashes.
            Char(char) => return write!(f, "{char:?}"),
            // NOTE: does this escape backslashes? since the input may contain \n, we should not escape backslashes.
            String(string) => return write!(f, "{string:?}"),
            Op(op) => op,
            Comment(comment) => return write!(f, "// {comment}"),
            LBrace => "{",
            RBrace => "}",
            LParen => "(",
            RParen => ")",
            LBracket => "[",
            RBracket => "]",
            Dot => ".",
            DotDot => "..",
            Comma => ",",
            Arrow => "->",
            Semi => ";",
            Colon => ":",
            ColonColon => "::",
            At => "@",
            Hashtag => "#",
            Dollar => "$",
        })
    }
}

use Token::*;

pub fn create_lexer<'src>()
-> impl Parser<'src, &'src str, Vec<Spanned<Token<'src>>>, LexErrorExtra<'src>> {
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
    let char = none_of("\\\'")
        .ignored()
        .or(escape)
        .to_slice()
        .delimited_by(just('\''), just('\''));

    let operator = choice((
        text::ascii::keyword("and"),
        text::ascii::keyword("or"),
        text::ascii::keyword("not"),
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
        text::ascii::keyword("use").to(Use),
        text::ascii::keyword("struct").to(Struct),
        text::ascii::keyword("enum").to(Enum),
        text::ascii::keyword("true").to(Bool(true)),
        text::ascii::keyword("false").to(Bool(false)),
        text::ascii::keyword("ref").to(Ref),
        text::ascii::keyword("copy").to(Copy),
        text::ascii::keyword("shallow_copy").to(ShallowCopy),
    ));
    let symbol = choice((
        just('{').to(LBrace),
        just('}').to(RBrace),
        just('(').to(LParen),
        just(')').to(RParen),
        just('[').to(LBracket),
        just(']').to(RBracket),
        just("..").to(DotDot).or(just('.').to(Dot)),
        just(',').to(Comma),
        just("->").to(Arrow),
        just(';').to(Semi),
        just("::").to(ColonColon),
        just(':').to(Colon),
        just('@').to(At),
        just('#').to(Hashtag),
        just('$').to(Dollar),
    ));
    choice((
        // NOTE: ordering is important to ensure the right tokens are lexed
        just("//").then(just(' ').or_not()).ignore_then(
            any()
                .and_is(text::newline().not())
                .repeated()
                .to_slice()
                .map(Comment),
        ),
        keyword,
        symbol,
        operator,
        text::ident().map(Ident),
        text::int(10).map(Int),
        char.map(Char),
        string.map(String),
    ))
    .map_with(|token, e| (token, e.span()))
    .padded()
    .repeated()
    .collect()
    .boxed()
}
