use crate::{Span, Spanned};

pub type LexError<'a> = (&'static str, Span<'a>);
pub type SpannedToken<'src> = Spanned<'src, Token<'src>>;

use Token::*;

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
    Char(char),
    String(&'src str),
    Comment(&'src str),
    // Unary and Binary Operators
    Plus,
    Minus, // Unary+Binary
    Star,
    Slash,
    Percent,
    LAngleEq,
    RAngleEq,
    EqEq,
    BangEq,
    Not, // Unary
    And,
    Or,
    // Assignment Operators
    Eq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    // Delimiters
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LAngle, // Also an operator
    RAngle, // Also an operator
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
    // Special
    Newline,
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
                | And
                | Or
                | Not
        )
    }

    pub fn is_operator(self) -> bool {
        matches!(
            self,
            Plus | Minus
                | Star
                | Slash
                | Percent
                | LAngleEq
                | RAngleEq
                | EqEq
                | BangEq
                | Not
                | And
                | Or
                | Eq
                | PlusEq
                | MinusEq
                | StarEq
                | SlashEq
                | PercentEq
                | LAngle
                | RAngle
        )
    }

    pub fn is_delimiter(self) -> bool {
        matches!(
            self,
            LBrace | RBrace | LParen | RParen | LBracket | RBracket | LAngle | RAngle
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
            Comment(comment) => return write!(f, "{comment}"),
            Plus => "+",
            Minus => "-",
            Star => "*",
            Slash => "/",
            Percent => "%",
            LAngleEq => "<=",
            RAngleEq => ">=",
            EqEq => "==",
            BangEq => "!=",
            Not => "!",
            And => "&&",
            Or => "||",
            Eq => "=",
            PlusEq => "+=",
            MinusEq => "-=",
            StarEq => "*=",
            SlashEq => "/=",
            PercentEq => "%=",
            LBrace => "{",
            RBrace => "}",
            LParen => "(",
            RParen => ")",
            LBracket => "[",
            RBracket => "]",
            LAngle => "<",
            RAngle => ">",
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
            Newline => "\\n",
        })
    }
}

struct Cursor<'lexer, 'src> {
    lexer: &'lexer mut Lexer<'src>,
    start: usize,
    end: usize,
}

impl<'lexer, 'src> Cursor<'lexer, 'src> {
    pub fn new(lexer: &'lexer mut Lexer<'src>) -> Self {
        Self {
            start: lexer.start,
            end: lexer.start,
            lexer,
        }
    }

    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    pub fn remaining(&self) -> &str {
        &self.lexer.src[self.end..]
    }

    pub fn skip(&mut self, len: usize) {
        debug_assert!(self.end + len <= self.lexer.src.len());
        self.end += len;
    }

    pub fn take(&mut self) -> Option<char> {
        self.peek().inspect(|c| {
            self.end += c.len_utf8();
        })
    }

    pub fn take_prefix(&mut self, prefix: &str) -> bool {
        let matches = self.remaining().starts_with(prefix);
        if matches {
            self.end += prefix.len();
        }
        matches
    }

    pub fn take_if(&mut self, pred: impl Fn(char) -> bool) -> bool {
        if let Some(c) = self.peek()
            && pred(c)
        {
            self.end += c.len_utf8();
            true
        } else {
            false
        }
    }

    pub fn take_while(&mut self, pred: impl Fn(char) -> bool) -> bool {
        let mut took_any = false;
        for c in self.lexer.src[self.end..].chars() {
            if !pred(c) {
                break;
            }
            self.end += c.len_utf8();
            took_any = true;
        }
        took_any
    }

    pub fn push_error(&mut self, message: &'static str, start: usize, end: usize) {
        let span = self.lexer.create_span(start, end);
        self.lexer.errors.push((message, span));
    }

    pub fn commit(self) -> (&'src str, Span<'src>) {
        self.lexer.start = self.end;
        let text = &self.lexer.src[self.start..self.end];
        let span = self.lexer.create_span(self.start, self.end);
        (text, span)
    }

    /// Returns the length of the discovered character escape (if any),
    /// and reports an error if encountered.
    fn take_escape<'a>(&mut self) -> Option<char> {
        let bytes = self.remaining().as_bytes();
        if bytes.is_empty() || bytes[0] != b'\\' {
            return None;
        }
        let Some(&escaped) = bytes.get(1) else {
            self.push_error(
                "Unexpected EOF; expected escape character",
                self.end,
                self.end + 1,
            );
            self.skip(1);
            return Some('?');
        };
        match escaped {
            b'\\' | b'"' | b'\'' | b'n' | b'r' | b't' => {
                self.skip(2);
                Some(match escaped {
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    _ => unreachable!(),
                })
            }
            b'u' => {
                let mut num_hex_digits = 0;
                for &b in &bytes[2..6] {
                    if !b.is_ascii_hexdigit() {
                        break;
                    }
                    num_hex_digits += 1;
                }
                if num_hex_digits < 4 {
                    self.push_error(
                        "Invalid unicode character; unicode characters require 4 hex digits",
                        self.end,
                        self.end + 2 + num_hex_digits,
                    );
                    self.skip(2 + num_hex_digits);
                    return Some('?');
                }
                let char = match char::from_u32(
                    u32::from_str_radix(str::from_utf8(&bytes[2..6]).unwrap(), 16).unwrap(),
                ) {
                    Some(char) => char,
                    None => {
                        self.push_error("Invalid unicode character escape", self.end, self.end + 6);
                        '?'
                    }
                };
                self.skip(6);
                Some(char)
            }
            _ => {
                self.push_error("Invalid character escape", self.end, self.end + 2);
                self.skip(2);
                Some('?')
            }
        }
    }
}

macro_rules! rule {
    ($self:expr, $cursor:ident, if $cond:expr, $then:block) => {{
        let mut $cursor = $self.cursor();
        let matches = $cond;
        if matches $then;
        matches
    }}
}

struct Lexer<'src> {
    src: &'src str,
    start: usize,
    tokens: Vec<SpannedToken<'src>>,
    filename: &'src str,
    errors: Vec<LexError<'src>>,
}

impl<'src> Lexer<'src> {
    fn cursor(&mut self) -> Cursor<'_, 'src> {
        Cursor::new(self)
    }

    fn keyword(&mut self, keyword: &str, token: Token<'static>) -> bool {
        rule!(self, cursor, if cursor.take_prefix(keyword)
            && cursor.peek().is_none_or(|c| !c.is_alphabetic()),
        {
            let (_, span) = cursor.commit();
            self.tokens.push((token, span));
        })
    }

    fn symbol(&mut self, symbol: &str, token: Token<'static>) -> bool {
        rule!(self, cursor, if cursor.take_prefix(symbol), {
            let (_, span) = cursor.commit();
            self.tokens.push((token, span));
        })
    }

    fn create_span(&self, start: usize, end: usize) -> Span<'src> {
        Span::new(start, end, self.filename, self.src)
    }

    // --- Inne-specific lexing functions ---

    fn inne_operator(&mut self) -> bool {
        self.keyword("and", And)
            || self.keyword("or", Or)
            || self.keyword("not", Not)
            || self.symbol("==", EqEq)
            || self.symbol("!=", BangEq)
            || self.symbol("<=", LAngleEq)
            || self.symbol(">=", RAngleEq)
            || self.symbol("+=", PlusEq)
            || self.symbol("-=", MinusEq)
            || self.symbol("*=", StarEq)
            || self.symbol("/=", SlashEq)
            || self.symbol("%=", PercentEq)
            || self.symbol("=", Eq)
            || self.symbol("%", Percent)
            || self.symbol("*", Star)
            || self.symbol("/", Slash)
            || self.symbol("+", Plus)
            || self.symbol("-", Minus)
            || self.symbol("<", LAngle)
            || self.symbol(">", RAngle)
    }

    fn inne_keyword(&mut self) -> bool {
        self.keyword("func", Func)
            || self.keyword("var", Var)
            || self.keyword("if", If)
            || self.keyword("else", Else)
            || self.keyword("for", For)
            || self.keyword("while", While)
            || self.keyword("loop", Loop)
            || self.keyword("break", Break)
            || self.keyword("continue", Continue)
            || self.keyword("return", Return)
            || self.keyword("in", In)
            || self.keyword("import", Import)
            || self.keyword("use", Use)
            || self.keyword("struct", Struct)
            || self.keyword("enum", Enum)
            || self.keyword("true", Bool(true))
            || self.keyword("false", Bool(false))
            || self.keyword("ref", Ref)
            || self.keyword("copy", Copy)
            || self.keyword("shallow_copy", ShallowCopy)
    }

    fn inne_symbol(&mut self) -> bool {
        self.symbol("{", LBrace)
            || self.symbol("}", RBrace)
            || self.symbol("(", LParen)
            || self.symbol(")", RParen)
            || self.symbol("[", LBracket)
            || self.symbol("]", RBracket)
            || self.symbol("..", DotDot)
            || self.symbol(".", Dot)
            || self.symbol(",", Comma)
            || self.symbol("->", Arrow)
            || self.symbol(";", Semi)
            || self.symbol("::", ColonColon)
            || self.symbol(":", Colon)
            || self.symbol("@", At)
            || self.symbol("#", Hashtag)
            || self.symbol("$", Dollar)
    }

    fn inne_ident(&mut self) -> bool {
        rule!(self, cursor, if cursor.take_if(|c| c.is_alphabetic() || c == '_'), {
            cursor.take_while(|c| c.is_alphanumeric() || c == '_');
            let (text, span) = cursor.commit();
            self.tokens.push((Token::Ident(text), span));
        })
    }

    fn inne_int(&mut self) -> bool {
        rule!(self, cursor, if cursor.take_while(|c| c.is_ascii_digit()), {
            let (text, span) = cursor.commit();
            self.tokens.push((Token::Int(text), span));
        })
    }

    fn inne_char(&mut self) -> bool {
        let mut cursor = self.cursor();
        if !cursor.take_if(|c| c == '\'') {
            return false;
        }
        let Some(char) = cursor.take_escape().or_else(|| cursor.take()) else {
            return false;
        };
        // An unfinished character is still treated as a character because its content is recoverable.
        let ends_with_apostrophe = cursor.take_if(|c| c == '\'');
        if !ends_with_apostrophe {
            cursor.push_error(
                "Found an unfinished character, add an apostrophe (') to complete it",
                cursor.start,
                cursor.end,
            );
        }
        let (_, span) = cursor.commit();
        // Remove the opening and closing apostrophes from the character's content.
        // TODO: just fill Token::Char with an actual char here (or '?' on failure)
        self.tokens.push((Token::Char(char), span));
        true
    }

    fn inne_string(&mut self) -> bool {
        let mut cursor = self.cursor();
        if !cursor.take_if(|c| c == '\"') {
            return false;
        }
        let ends_with_quote = loop {
            if cursor.take_escape().is_some() {
                // OK
            } else if cursor.take_if(|c| c == '"') {
                break true;
            } else if cursor.take().is_none() {
                cursor.push_error(
                    "Found an unfinished string, add a quote '\"' to complete it",
                    cursor.start,
                    cursor.end,
                );
                break false;
            }
        };
        let (mut text, span) = cursor.commit();
        // Remove the opening and closing quotes from the string's content.
        text = &text[1..];
        if ends_with_quote {
            text = &text[..text.len() - 1];
        }
        let token = Token::String(text);
        self.tokens.push((token, span));
        true
    }

    fn inne_comment(&mut self) -> bool {
        rule!(self, cursor, if cursor.take_prefix("//"), {
            cursor.take_while(|c| c != '\n');
            let (text, span) = cursor.commit();
            self.tokens.push((Token::Comment(text), span));
        })
    }

    fn inne_whitespace(&mut self) {
        rule!(self, cursor, if cursor.take_while(|c| c.is_ascii_whitespace() && c != '\n'), {
            cursor.commit();
        });
    }

    fn inne_newline(&mut self) -> bool {
        rule!(self, cursor, if cursor.take_if(|c| c == '\n'), {
            let (_, span) = cursor.commit();
            self.tokens.push((Token::Newline, span));
        })
    }

    /// Returns `false` at end-of-file.
    /// It does not return `false` on failure as it recovers from errors.
    fn inne_token(&mut self) -> bool {
        let start_invalid = self.start;
        let mut end_invalid = self.start;
        let mut success = false; // Lexed anything

        self.inne_whitespace(); // Skip whitespace
        while self.start < self.src.len() {
            if self.inne_newline()
                || self.inne_comment()
                || self.inne_symbol()
                || self.inne_operator()
                || self.inne_keyword()
                || self.inne_ident()
                || self.inne_int()
                || self.inne_char()
                || self.inne_string()
            {
                success = true;
                break;
            }
            self.start += 1;
            end_invalid += 1;
        }
        if start_invalid < end_invalid {
            self.errors.push((
                "found invalid character(s)",
                self.create_span(start_invalid, end_invalid),
            ));
        }
        success
    }
}

fn filter_newlines<'src>(tokens: &mut Vec<SpannedToken<'src>>) {
    let mut dst = 0;
    for src in 0..tokens.len() {
        let (token, _) = tokens[src];
        if token != Token::Newline {
            tokens[dst] = tokens[src];
            dst += 1;
            continue;
        }
        if src == 0 {
            continue;
        }
        let (prev, _) = tokens[src - 1];
        if matches!(
            prev,
            RBrace
                | RParen
                | RBracket
                | Return
                | Continue
                | Break
                | Ident(_)
                | Int(_)
                | Bool(_)
                | String(_)
                | Char(_)
        ) {
            tokens[dst] = tokens[src];
            dst += 1;
            continue;
        }
        if src + 1 < tokens.len() && tokens[src + 1].0.is_operator() {
            tokens[dst] = tokens[src];
            dst += 1;
            continue;
        }
    }
    tokens.truncate(dst);
}

/// Note that this will still produce a list of tokens even when errors have been encountered.
pub fn lex<'src>(
    src: &'src str,
    filename: &'src str,
) -> (Vec<SpannedToken<'src>>, Vec<LexError<'src>>) {
    let mut lexer = Lexer {
        src,
        start: 0,
        tokens: Vec::new(),
        filename,
        errors: Vec::new(),
    };
    while lexer.inne_token() {}
    debug_assert_eq!(lexer.start, src.len());
    filter_newlines(&mut lexer.tokens);
    (lexer.tokens, lexer.errors)
}
