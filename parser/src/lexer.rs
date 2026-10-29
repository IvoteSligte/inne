use crate::{Span, Spanned};

pub type LexError = (&'static str, Span);

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
            Comment(comment) => return write!(f, "{comment}"),
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

    pub fn push_error(&mut self, error: LexError) {
        self.lexer.errors.push(error);
    }

    pub fn commit(self) -> (&'src str, Span) {
        self.lexer.start = self.end;
        let text = &self.lexer.src[self.start..self.end];
        let span = (self.start..self.end).into();
        (text, span)
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

/// Returns the length of the discovered character escape (if any),
/// as well as an error if relevant.
fn check_escape(str: &str) -> (usize, Option<LexError>) {
    let bytes = str.as_bytes();
    if bytes.is_empty() || bytes[0] != b'\\' {
        return (0, None);
    }
    let Some(escaped) = bytes.get(1) else {
        return (1, None);
    };
    match escaped {
        b'\\' | b'/' | b'"' | b'b' | b'f' | b'n' | b'r' | b't' => (2, None),
        b'u' => {
            let mut num_hex_digits = 0;
            for &b in &bytes[2..6] {
                if !b.is_ascii_hexdigit() {
                    break;
                }
                num_hex_digits += 1;
            }
            if num_hex_digits < 4 {
                let error = (
                    "Invalid unicode character; unicode characters require 4 hex digits",
                    (0..2 + num_hex_digits).into(),
                );
                return (2 + num_hex_digits, Some(error));
            }
            (6, None)
        }
        _ => {
            let error = ("Invalid character escape", (0..2).into());
            (2, Some(error))
        }
    }
}

struct Lexer<'src> {
    src: &'src str,
    start: usize,
    tokens: Vec<Spanned<Token<'src>>>,
    errors: Vec<LexError>,
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

    // --- Inne-specific lexing functions ---

    fn inne_operator(&mut self) -> bool {
        self.keyword("and", Op("and"))
            || self.keyword("or", Op("or"))
            || self.keyword("not", Op("not"))
            || self.symbol("==", Op("=="))
            || self.symbol("!=", Op("!="))
            || self.symbol("<=", Op("<="))
            || self.symbol(">=", Op(">="))
            || self.symbol("+=", Op("+="))
            || self.symbol("-=", Op("-="))
            || self.symbol("*=", Op("*="))
            || self.symbol("/=", Op("/="))
            || self.symbol("%=", Op("%="))
            || self.symbol("=", Op("="))
            || self.symbol("%", Op("%"))
            || self.symbol("*", Op("*"))
            || self.symbol("/", Op("/"))
            || self.symbol("+", Op("+"))
            || self.symbol("-", Op("-"))
            || self.symbol("<", Op("<"))
            || self.symbol(">", Op(">"))
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
        let (len, maybe_error) = check_escape(cursor.remaining());
        if let Some(error) = maybe_error {
            cursor.push_error(error);
        }
        if len != 0 {
            cursor.skip(len);
        } else if cursor.take().is_none() {
            return false;
        }
        // An unfinished character is still treated as a character because its content is recoverable.
        let ends_with_apostrophe = cursor.take_if(|c| c == '\'');
        if !ends_with_apostrophe {
            cursor.push_error((
                "found an unfinished character, add an apostrophe (') to complete it",
                (cursor.start..cursor.end).into(),
            ));
        }
        let (mut text, span) = cursor.commit();
        // Remove the opening and closing apostrophes from the character's content.
        text = &text[1..];
        if ends_with_apostrophe {
            text = &text[..text.len() - 1];
        }
        // TODO: just fill Token::Char with an actual char here (or '?' on failure)
        self.tokens.push((Token::Char(text), span));
        true
    }

    fn inne_string(&mut self) -> bool {
        let mut cursor = self.cursor();
        if !cursor.take_if(|c| c == '\"') {
            return false;
        }
        let ends_with_quote = loop {
            let (len, maybe_error) = check_escape(cursor.remaining());
            if let Some(error) = maybe_error {
                cursor.push_error(error);
            }
            if len != 0 {
                cursor.skip(len);
            } else if cursor.take_if(|c| c == '"') {
                break true;
            } else if cursor.take().is_none() {
                cursor.push_error((
                    "found an unfinished string, add a quote '\"' to complete it",
                    (cursor.start..cursor.end).into(),
                ));
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
        rule!(self, cursor, if cursor.take_while(|c| c.is_ascii_whitespace()), {
            cursor.commit();
        });
    }

    /// Returns `false` at end-of-file.
    /// It does not return `false` on failure as it recovers from errors.
    fn inne_token(&mut self) -> bool {
        let start_invalid = self.start;
        let mut end_invalid = self.start;
        let mut success = false; // Lexed anything

        self.inne_whitespace(); // Skip whitespace
        while self.start < self.src.len() {
            if self.inne_comment()
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
                (start_invalid..end_invalid).into(),
            ));
        }
        success
    }
}

/// Note that this will still produce a list of tokens even when errors have been encountered.
pub fn lex<'src>(src: &'src str) -> (Vec<Spanned<Token<'src>>>, Vec<LexError>) {
    let mut lexer = Lexer {
        src,
        start: 0,
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    while lexer.inne_token() {}
    debug_assert_eq!(lexer.start, src.len());
    (lexer.tokens, lexer.errors)
}
