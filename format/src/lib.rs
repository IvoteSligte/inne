use std::fmt::{Result, Write};

use indentify::*;
use inne::{Spanned, Token, ast::*, lex, parse};

// TODO: round-trip tests
// TODO: keep comments written on the same line as a statement on the same line after formatting

// TODO: Determine if an extra newline should be inserted based on the source.
//       If statements were in the input with (at least) one line in between them,
//       then they should have a line in between them in the formatted code.

struct Writer<'src> {
    tokens: &'src [Spanned<Token<'src>>],
    output: IndentWriter<'static, String>,
}

impl Writer<'_> {
    pub fn into_output(mut self) -> std::result::Result<String, std::fmt::Error> {
        for token in self.tokens {
            self.handle_source_token(*token)?;
        }
        Ok(self.output.child)
    }

    fn handle_source_token(&mut self, token: Spanned<Token<'_>>) -> Result {
        if let (Token::Comment(comment), _) = token {
            if self.output.child.ends_with(|c: char| !c.is_whitespace()) {
                self.output.write_str(" ")?;
            }
            writeln!(self.output, "{comment}")?;
        }
        Ok(())
    }

    fn skip_to(&mut self, token: Token<'_>) -> Result {
        // Some tokens are not guaranteed to appear in the input, so we ignore them.
        if matches!(
            token,
            Token::Semi | Token::Comma | Token::Arrow | Token::Colon
        ) {
            return Ok(());
        }
        let mid = self
            .tokens
            .iter()
            .position(|(other_token, _)| *other_token == token)
            .unwrap_or_else(|| {
                panic!("Failed to find token {token:?} in the input");
            });
        let (tokens_before, tokens_after) = self.tokens.split_at(mid);
        self.tokens = tokens_after;

        for token in tokens_before {
            self.handle_source_token(*token)?;
        }
        Ok(())
    }

    fn write(&mut self, token: Token<'_>) -> Result {
        write!(&mut self.output, "{token}")
    }

    pub fn put(&mut self, token: Token<'_>) -> Result {
        self.skip_to(token)?;
        self.write(token)
    }

    pub fn put_indent_lbrace(&mut self) -> Result {
        self.skip_to(Token::LBrace)?;
        write!(&mut self.output, "{INDENT}")?;
        self.write(Token::LBrace)
    }

    pub fn put_dedent_rbrace(&mut self) -> Result {
        self.skip_to(Token::RBrace)?;
        write!(&mut self.output, "{DEDENT}")?;
        self.write(Token::RBrace)
    }

    pub fn put_semi(&mut self) -> Result {
        self.put(Token::Semi)
    }

    pub fn put_comma(&mut self) -> Result {
        self.put(Token::Comma)
    }

    pub fn put_newline(&mut self) -> Result {
        self.output.write_str("\n")
    }

    pub fn put_display(&mut self, display: impl std::fmt::Display) -> Result {
        write!(&mut self.output, "{display}")
    }

    pub fn put_space(&mut self) -> Result {
        self.output.write_str(" ")
    }

    pub fn put_name(&mut self, (name, _): Spanned<&str>) -> Result {
        self.put(Token::Ident(name))
    }

    pub fn put_keyword(&mut self, keyword: Token<'_>) -> Result {
        self.put(keyword)?;
        self.put_space()
    }
}

trait Format {
    fn fmt(&self, f: &mut Writer<'_>) -> Result;
}

impl<T: Format> Format for &T {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        (**self).fmt(f)
    }
}

impl<T: Format> Format for Spanned<T> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.0.fmt(f)
    }
}

impl Format for Name<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_name(*self)
    }
}

macro_rules! delegate {
    ($self:ident, $f:ident, $($variant:ident),*; $($variant2:ident($value:ident) => $expr:expr),* $(,)?) => {
        match $self {
            $($variant(item) => item.fmt($f),)*
            $($variant2($value) => $expr,)*
        }
    }
}

impl Format for Ast<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_newline()?;
        for stmt in &self.stmts {
            stmt.fmt(f)?;
            f.put_newline()?;
            f.put_newline()?;
        }
        Ok(())
    }
}

impl Format for Stmt<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        use Stmt::*;
        delegate!(self, f,
            Func,
            Struct,
            Enum,
            VarDecl,
            Assign,
            If,
            For,
            While,
            Loop;
            Expr(expr) => {
                expr.fmt(f)?;
                f.put_semi()
            },
            Break(_span) => {
                f.put(Token::Break)?;
                f.put_semi()
            },
            Continue(_span) => {
                f.put(Token::Continue)?;
                f.put_semi()
            },
            Return(value) => {
                match &value.0 {
                    Some((value, _)) => {
                        f.put_keyword(Token::Return)?;
                        value.fmt(f)?;
                        f.put_semi()
                    }
                    None => {
                        f.put(Token::Return)?;
                        f.put_semi()
                    }
                }
            }
        )
    }
}

impl Format for Block<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        if self.is_empty() {
            f.put(Token::LBrace)?;
            return f.put(Token::RBrace);
        }
        f.put_indent_lbrace()?;
        f.put_newline()?;
        for stmt in self {
            stmt.fmt(f)?;
            f.put_newline()?;
        }
        f.put_dedent_rbrace()
    }
}

impl Format for Type<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.name.fmt(f)?;
        if !self.generics.is_empty() {
            f.put(Token::Op("<"))?;
            write_comma_separated(f, &self.generics)?;
            f.put(Token::Op(">"))?;
        }
        Ok(())
    }
}

impl Format for Func<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::Func)?;
        self.name.fmt(f)?;
        if !self.generics.is_empty() {
            f.put(Token::Op("<"))?;
            write_comma_separated(f, &self.generics)?;
            f.put(Token::Op(">"))?;
        }
        f.put(Token::LParen)?;
        write_comma_separated(f, &self.params)?;
        f.put(Token::RParen)?;
        f.put_space()?;
        if let Some(r#type) = self.return_type.as_ref() {
            f.put(Token::Arrow)?;
            f.put_space()?;
            r#type.fmt(f)?;
            f.put_space()?;
        }
        self.body.fmt(f)
    }
}

impl Format for Param<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        write_key_value(f, self.name, &self.r#type)
    }
}

impl Format for Struct<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::Struct)?;
        self.name.fmt(f)?;
        if !self.fields.is_empty() {
            f.put_indent_lbrace()?;
            for field in &self.fields {
                field.fmt(f)?;
                f.put_comma()?;
                f.put_newline()?;
            }
            f.put_dedent_rbrace()
        } else {
            f.put(Token::LBrace)?;
            f.put(Token::RBrace)
        }
    }
}

impl Format for Field<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        write_key_value(f, self.name, &self.r#type)
    }
}

impl Format for Enum<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::Enum)?;
        self.name.fmt(f)?;
        if !self.variants.is_empty() {
            f.put_indent_lbrace()?;
            for variant in &self.variants {
                variant.fmt(f)?;
                f.put_comma()?;
                f.put_newline()?;
            }
            f.put_dedent_rbrace()
        } else {
            f.put(Token::LBrace)?;
            f.put(Token::RBrace)
        }
    }
}

impl Format for Variant<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.name.fmt(f)?;
        if let Some(payload) = self.payload.as_ref() {
            f.put(Token::LParen)?;
            payload.fmt(f)?;
            f.put(Token::RParen)?;
        }
        Ok(())
    }
}

impl Format for VarDecl<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::Var)?;
        self.name.fmt(f)?;
        f.put_space()?;
        f.put(Token::Op("="))?;
        f.put_space()?;
        self.value.fmt(f)?;
        f.put_semi()
    }
}

impl Format for Expr<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        use Expr::*;
        delegate!(
            self,
            f,
            Binary,
            Unary,
            Call,
            EmptyList,
            List,
            EmptyMap,
            Map,
            StructLiteral,
            VariantLiteral,
            Index,
            Member,
            Range;
            Var(var) => f.put(Token::Ident(var)),
            Int(int) => f.put(Token::Int(int)),
            Bool(bool) => f.put(Token::Bool(*bool)),
            String(string) => f.put(Token::String(string)),
            Char(char) => f.put(Token::Char(char)),
        )
    }
}

/// Higher precedence means higher "binding power".
/// The returned number is only useful for comparisons with other return values.
pub fn precedence(op: BinaryOp) -> u8 {
    use BinaryOp::*;
    match op {
        Mul | Div | Mod => 9,
        Add | Sub => 8,
        Eq | NotEq => 7,
        Lt | Gt | Leq | Geq => 6,
        And => 5,
        Or => 4,
    }
}

impl Format for Binary<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        let op = self.op.0;
        // All operators are assumed to be left-associative.
        let left_needs_parens = matches!(
            self.left.0,
            Expr::Binary(Binary { op: (left_op, _), ..}) if precedence(left_op) < precedence(op)
        ) || matches!(self.left.0, Expr::Range(_));
        let right_needs_parens = matches!(
            self.right.0,
            Expr::Binary(Binary { op: (right_op, _), ..}) if precedence(right_op) <= precedence(op)
        ) || matches!(self.right.0, Expr::Range(_));
        {
            if left_needs_parens {
                f.put(Token::LParen)?;
            }
            self.left.fmt(f)?;
            if left_needs_parens {
                f.put(Token::RParen)?;
            }
        }
        f.put_space()?;
        self.op.0.fmt(f)?;
        f.put_space()?;
        {
            if right_needs_parens {
                f.put(Token::LParen)?;
            }
            self.right.fmt(f)?;
            if right_needs_parens {
                f.put(Token::RParen)?;
            }
        }
        Ok(())
    }
}

impl Format for BinaryOp {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_display(self)
    }
}

impl Format for Unary<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.op.0.fmt(f)?;
        let needs_parens = matches!(
            &self.expr.0,
            Expr::Binary(_) | Expr::Unary(_) | Expr::Range(_)
        );
        if needs_parens {
            f.put(Token::LParen)?;
        }
        self.expr.fmt(f)?;
        if needs_parens {
            f.put(Token::RParen)?;
        }
        Ok(())
    }
}

impl Format for UnaryOp {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_display(self)
    }
}

impl Format for Call<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.name.fmt(f)?;
        f.put(Token::LParen)?;
        write_comma_separated(f, &self.args)?;
        f.put(Token::RParen)
    }
}

impl Format for EmptyList<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put(Token::LBracket)?;
        f.put(Token::Op("<"))?;
        self.element_type.fmt(f)?;
        f.put(Token::Op(">"))?;
        f.put(Token::RBracket)
    }
}

impl Format for List<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put(Token::LBracket)?;
        write_comma_separated(f, &self.elements)?;
        f.put(Token::RBracket)
    }
}

impl Format for EmptyMap<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put(Token::LBrace)?;
        f.put(Token::Op("<"))?;
        self.key_type.fmt(f)?;
        f.put_comma()?;
        f.put_space()?;
        self.value_type.fmt(f)?;
        f.put(Token::Op(">"))?;
        f.put(Token::RBrace)
    }
}

impl Format for Map<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put(Token::LBrace)?;
        write_comma_separated(f, &self.pairs)?;
        f.put(Token::RBrace)
    }
}

impl Format for Pair<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        write_key_value(f, &self.key, &self.value)
    }
}

impl Format for StructLiteral<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.name.fmt(f)?;
        f.put_space()?;
        f.put(Token::LBrace)?;
        f.put_space()?;
        write_comma_separated(f, &self.fields)?;
        f.put_space()?;
        f.put(Token::RBrace)
    }
}

impl Format for FieldLiteral<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        write_key_value(f, self.name, &self.value)
    }
}

impl Format for VariantLiteral<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.enum_name.fmt(f)?;
        f.put(Token::ColonColon)?;
        self.name.fmt(f)?;
        if let Some(payload) = self.payload.as_ref() {
            f.put(Token::LParen)?;
            payload.fmt(f)?;
            f.put(Token::RParen)?;
        }
        Ok(())
    }
}

impl Format for Index<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        let needs_parens = matches!(
            self.expr.0,
            Expr::Binary(_) | Expr::Unary(_) | Expr::Range(_),
        );
        if needs_parens {
            f.put(Token::LParen)?;
        }
        self.expr.fmt(f)?;
        if needs_parens {
            f.put(Token::RParen)?;
        }
        f.put(Token::LBracket)?;
        self.index.fmt(f)?;
        f.put(Token::RBracket)
    }
}

impl Format for Member<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        let needs_parens = matches!(
            self.expr.0,
            Expr::Binary(_) | Expr::Unary(_) | Expr::Range(_),
        );
        if needs_parens {
            f.put(Token::LParen)?;
        }
        self.expr.fmt(f)?;
        if needs_parens {
            f.put(Token::RParen)?;
        }
        f.put(Token::Dot)?;
        self.member.fmt(f)
    }
}

impl Format for Range<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        let start_needs_parens = matches!(self.start.0, Expr::Range(_));
        let end_needs_parens = matches!(self.start.0, Expr::Range(_));

        {
            if start_needs_parens {
                f.put(Token::LParen)?;
            }
            self.start.fmt(f)?;
            if start_needs_parens {
                f.put(Token::RParen)?;
            }
        }
        f.put(Token::DotDot)?;
        {
            if end_needs_parens {
                f.put(Token::LParen)?;
            }
            self.end.fmt(f)?;
            if end_needs_parens {
                f.put(Token::RParen)?;
            }
        }
        Ok(())
    }
}

impl Format for Assign<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        self.target.fmt(f)?;
        f.put_space()?;
        self.op.fmt(f)?;
        f.put_space()?;
        self.value.fmt(f)?;
        f.put_semi()
    }
}

impl Format for AssignOp {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_display(self)
    }
}

impl Format for If<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::If)?;
        self.cond.fmt(f)?;
        f.put_space()?;
        self.then.fmt(f)?;
        for (cond, then) in &self.else_ifs {
            f.put_space()?;
            f.put_keyword(Token::Else)?;
            f.put_keyword(Token::If)?;
            cond.fmt(f)?;
            f.put_space()?;
            then.fmt(f)?;
        }
        if let Some(r#else) = &self.r#else {
            f.put_space()?;
            f.put_keyword(Token::Else)?;
            f.put_space()?;
            r#else.fmt(f)?;
        }
        Ok(())
    }
}

impl Format for For<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::For)?;
        self.var.fmt(f)?;
        f.put_space()?;
        f.put(Token::In)?;
        f.put_space()?;
        self.iter.fmt(f)?;
        f.put_space()?;
        self.body.fmt(f)?;
        Ok(())
    }
}

impl Format for While<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::While)?;
        self.cond.fmt(f)?;
        f.put_space()?;
        self.body.fmt(f)?;
        Ok(())
    }
}

impl Format for Loop<'_> {
    fn fmt(&self, f: &mut Writer<'_>) -> Result {
        f.put_keyword(Token::Loop)?;
        self.body.fmt(f)?;
        Ok(())
    }
}

/// Returns `None` on parsing failure and `Some(Err(_))` on write failure.
pub fn format(source: &str, mut dest: impl Write) -> Option<Result> {
    let (maybe_tokens, errors) = lex(source);
    if !errors.is_empty() {
        return None;
    }
    let Some(tokens) = maybe_tokens else {
        return None;
    };
    let (maybe_ast, errors) = parse(&tokens, source, "file.inne");
    if !errors.is_empty() {
        return None;
    }
    let Some(ast) = maybe_ast else {
        return None;
    };
    let output = String::with_capacity(source.len());
    let mut writer = Writer {
        tokens: &tokens,
        output: IndentWriter::new("    ", output),
    };
    if let Err(error) = ast.fmt(&mut writer) {
        return Some(Err(error));
    }
    Some(
        writer
            .into_output()
            .and_then(|string| dest.write_str(&string)),
    )
}

fn write_comma_separated(
    f: &mut Writer<'_>,
    items: impl IntoIterator<Item = impl Format>,
) -> Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.put_comma()?;
            f.put_space()?;
        }
        item.fmt(f)?;
    }
    Ok(())
}

fn write_key_value(f: &mut Writer<'_>, key: impl Format, value: impl Format) -> Result {
    key.fmt(f)?;
    f.put(Token::Colon)?;
    f.put_space()?;
    value.fmt(f)?;
    Ok(())
}
