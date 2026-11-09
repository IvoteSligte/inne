use Token::*;

use crate::escaped_byte_to_char;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token<'src> {
    // Keywords
    Func,
    Var,
    If,
    Else,
    Match,
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
    String(InneStr<'src>),       // "text"
    StringStart(InneStr<'src>),  // "text{
    StringMiddle(InneStr<'src>), // }text{
    StringEnd(InneStr<'src>),    // }text"
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
                | Match
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
            Match => "match",
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
            StringStart(text) => return write!(f, "\"{}{{", text.original()),
            StringMiddle(text) => return write!(f, "}}{}{{", text.original()),
            StringEnd(text) => return write!(f, "}}{}\"", text.original()),
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
            Colon => ":",
            ColonColon => "::",
            At => "@",
            Hashtag => "#",
            Dollar => "$",
            Newline => "\\n",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InneStr<'src>(pub(crate) &'src str);

impl InneStr<'_> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn chars(&self) -> impl Iterator<Item = char> {
        let mut s = self.0;
        std::iter::from_fn(move || {
            let Some(c) = s.chars().next() else {
                return None;
            };
            let bytes = s.as_bytes();
            if c == '\\'
                && let Some(&escaped) = bytes.get(1)
            {
                s = &s[2..];
                return Some(escaped_byte_to_char(escaped).unwrap());
            }
            s = &s[c.len_utf8()..];
            Some(c)
        })
    }

    pub fn original(&self) -> &str {
        self.0
    }
}

impl std::fmt::Debug for InneStr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
