use crate::{Span, Spanned, SpannedToken, Token, ast::*};

// TODO: error recovery and consequently prevent duplicate error messages
// TODO: list/map/struct/enum pattern matching in variable declarations

pub type ParseError<'a> = (String, Span<'a>);

const PARENS: (Token<'static>, Token<'static>) = (Token::LParen, Token::RParen);
const BRACKETS: (Token<'static>, Token<'static>) = (Token::LBracket, Token::RBracket);
const BRACES: (Token<'static>, Token<'static>) = (Token::LBrace, Token::RBrace);
const ANGLES: (Token<'static>, Token<'static>) = (Token::LAngle, Token::RAngle);

struct Parser<'parent, 'src> {
    src: &'parent [SpannedToken<'src>],
    errors: &'parent mut Vec<ParseError<'src>>,
    end: usize,
}

impl<'parent, 'src> Parser<'parent, 'src> {
    fn atomic<T>(&mut self, func: impl FnOnce(&mut Parser<'_, 'src>) -> Option<T>) -> Option<T> {
        let mut _self = Parser {
            end: self.end,
            src: self.src,
            errors: self.errors,
        };
        func(&mut _self).inspect(|_| {
            self.end = _self.end;
        })
    }

    fn spanned<T>(
        &mut self,
        func: impl FnOnce(&mut Parser<'_, 'src>) -> Option<T>,
    ) -> Option<Spanned<'src, T>> {
        let start_span = self.src.get(self.end)?.1;
        func(self).map(|value| {
            let end_span = self.src[self.end - 1].1;
            let span = start_span.union(end_span);
            (value, span)
        })
    }

    fn atomic_spanned<T>(
        &mut self,
        func: impl FnOnce(&mut Parser<'_, 'src>) -> Option<T>,
    ) -> Option<Spanned<'src, T>> {
        self.atomic(|s| s.spanned(func))
    }

    /// This also parses single instances of `func`.
    fn parse_binary(
        &mut self,
        op_map: impl Fn(Token) -> Option<BinaryOp>,
        func: impl Fn(&mut Parser<'_, 'src>) -> Option<SpannedExpr<'src>>,
    ) -> Option<SpannedExpr<'src>> {
        let mut left = func(self)?;
        while let Some(op) = self.take_if_map(&op_map) {
            let right = func(self)?;
            let span = left.1.union(right.1);
            left = (
                Expr::Binary(Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }),
                span,
            );
        }
        Some(left)
    }

    fn must<T>(
        &mut self,
        label: &'static str,
        func: impl FnOnce(&mut Parser<'_, 'src>) -> Option<T>,
    ) -> Option<T> {
        let maybe = self.atomic(func);
        if maybe.is_none() && !self.src.is_empty() && self.errors.is_empty() {
            let span = match self.src.get(self.end) {
                Some((_, span)) => *span,
                None => {
                    let (_, last_span) = self.src.last().unwrap();
                    Span {
                        start: last_span.end - 1,
                        ..*last_span
                    }
                }
            };
            self.errors.push((format!("Failed to parse {label}"), span))
        }
        maybe
    }

    fn must_token(&mut self, token: Token<'src>) -> Option<SpannedToken<'src>> {
        let maybe = self.take_if_eq(token);
        if maybe.is_none() {
            let span = self.src[self.end].1;
            self.errors
                .push((format!("Expected token '{token}'"), span));
        }
        maybe
    }

    fn peek(&self) -> Option<SpannedToken<'src>> {
        self.remaining().iter().copied().next()
    }

    fn remaining(&self) -> &[SpannedToken<'src>] {
        &self.src[self.end..]
    }

    #[must_use]
    fn take_if_map<T>(
        &mut self,
        pred: impl Fn(Token<'src>) -> Option<T>,
    ) -> Option<Spanned<'src, T>> {
        if !self.errors.is_empty() {
            return None;
        }
        if let Some((token, span)) = self.peek()
            && let Some(value) = pred(token)
        {
            self.end += 1;
            Some((value, span))
        } else {
            None
        }
    }

    #[must_use]
    fn take_if(&mut self, pred: impl Fn(Token<'src>) -> bool) -> Option<SpannedToken<'src>> {
        self.take_if_map(|t| pred(t).then_some(t))
    }

    #[must_use]
    fn take_if_eq(&mut self, token: Token<'src>) -> Option<SpannedToken<'src>> {
        self.take_if(|t| t == token)
    }

    fn delimited_by<T>(
        &mut self,
        (opening, closing): (Token<'src>, Token<'src>),
        func: impl Fn(&mut Self) -> T,
    ) -> Option<T> {
        debug_assert_ne!(opening, closing); // This is basically never desirable, but an easy mistake to make.
        self.take_if_eq(opening)?;
        let value = func(self);
        let _ = self.must("closing delimiter", |s| s.take_if_eq(closing));
        Some(value)
    }

    /// This function is non-atomic.
    /// If `func` consumes a token and still returns `None`, then that token is permanently gone.
    fn repeated<T>(&mut self, func: impl Fn(&mut Self) -> Option<T>) -> Vec<T> {
        (0..).map_while(|_| func(self)).collect()
    }

    /// This function is non-atomic.
    /// If `func` consumes a token and still returns `None`, then that token is permanently gone.
    ///
    /// Allows a trailing separator.
    fn separated_by<T>(
        &mut self,
        separator: Token<'src>,
        func: impl Fn(&mut Self) -> Option<T>,
    ) -> Vec<T> {
        let Some(first) = func(self) else {
            return Vec::new();
        };
        let mut values = vec![first];
        // This greedily consumes the separator before querying `func`.
        while self.take_if_eq(separator).is_some()
            && let Some(value) = func(self)
        {
            values.push(value);
        }
        values
    }

    /// This function is non-atomic.
    /// If `func` consumes a token and still returns `None`, then that token is permanently gone.
    ///
    /// Allows a trailing comma.
    fn separated_by_comma<T>(&mut self, func: impl Fn(&mut Self) -> Option<T>) -> Vec<T> {
        self.separated_by(Token::Comma, func)
    }

    // --- Inne-specific methods ---

    fn ident(&mut self) -> Option<Spanned<'src, &'src str>> {
        let (token, span) = self.take_if(|t| matches!(t, Token::Ident(_)))?;
        match token {
            Token::Ident(name) => Some((name, span)),
            _ => unreachable!(),
        }
    }

    fn r#type(&mut self) -> Option<SpannedType<'src>> {
        self.atomic_spanned(|s| {
            let name = s.ident()?;
            let generics = s
                .delimited_by(ANGLES, |s| s.separated_by_comma(Parser::r#type))
                .unwrap_or_default();
            Some(Type { name, generics })
        })
    }

    fn param(&mut self) -> Option<Param<'src>> {
        let name = self.ident()?;
        let _ = self.take_if_eq(Token::Colon);
        let r#type = self.must("parameter type", |s| s.r#type())?;
        Some(Param { name, r#type })
    }

    fn field(&mut self) -> Option<Field<'src>> {
        let name = self.ident()?;
        let _ = self.take_if_eq(Token::Colon);
        let r#type = self.must("field type", |s| s.r#type())?;
        Some(Field { name, r#type })
    }

    fn variant(&mut self) -> Option<Variant<'src>> {
        let name = self.ident()?;
        self.atomic(|s| {
            let payload = s
                .delimited_by(PARENS, |s| s.must("payload type", |s| s.r#type()))
                .flatten();
            Some(Variant { name, payload })
        })
    }

    fn generics(&mut self) -> Vec<Name<'src>> {
        self.delimited_by(ANGLES, |s| s.separated_by_comma(Parser::ident))
            .unwrap_or_default()
    }

    fn block(&mut self) -> Option<Vec<Stmt<'src>>> {
        self.atomic(|s| s.delimited_by(BRACES, |s| s.repeated(Parser::stmt)))
    }

    fn import(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::Import)?;
        let path = self.must("import path", |s| {
            s.spanned(|s| {
                let path = s.separated_by(Token::ColonColon, |s| s.ident());
                (!path.is_empty()).then_some(path)
            })
        })?;
        Some(Stmt::Import(Import { path }))
    }

    fn func(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::Func)?;
        let name = self.must("function name", |s| s.ident())?;
        let generics = self.generics();
        let params = self.must("function parameters", |s| {
            s.delimited_by(PARENS, |s| s.separated_by_comma(Parser::param))
        })?;
        let _ = self.take_if_eq(Token::Arrow);
        let return_type = self.r#type();
        let body = self.must("function body", |s| s.block())?;
        Some(Stmt::Func(Func {
            name,
            generics,
            params,
            return_type,
            body,
        }))
    }

    fn r#struct(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::Struct)?;
        let name = self.must("struct name", |s| s.ident())?;
        let generics = self.generics();
        let fields = self.must("struct fields", |s| {
            s.delimited_by(BRACES, |s| s.separated_by_comma(Parser::field))
        })?;
        Some(Stmt::Struct(Struct {
            name,
            generics,
            fields,
        }))
    }

    fn r#enum(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::Enum)?;
        let name = self.must("enum name", |s| s.ident())?;
        let generics = self.generics();
        let variants = self.must("enum variants", |s| {
            s.delimited_by(BRACES, |s| s.separated_by_comma(Parser::variant))
        })?;
        Some(Stmt::Enum(Enum {
            name,
            generics,
            variants,
        }))
    }

    fn var_decl(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::Var)?;
        let name = self.must("variable name", |s| s.ident())?;
        self.must("=", |s| s.take_if_eq(Token::Eq))?;
        let value = self.must("value", |s| s.expr())?;
        Some(Stmt::VarDecl(VarDecl { name, value }))
    }

    fn if_stmt_head(&mut self) -> Option<(SpannedExpr<'src>, Block<'src>)> {
        self.take_if_eq(Token::If)?;
        let cond = self.must("condition", |s| s.expr())?;
        let then = self.must("block", |s| s.block())?;
        Some((cond, then))
    }

    fn r#if(&mut self) -> Option<Stmt<'src>> {
        let (cond, then) = self.if_stmt_head()?;
        let mut else_ifs = Vec::new();
        let mut r#else = None;
        while self.take_if_eq(Token::Else).is_some() {
            if let Some(else_if) = self.if_stmt_head() {
                else_ifs.push(else_if);
            } else {
                r#else = Some(self.must("block", |s| s.block())?);
                break;
            }
        }
        Some(Stmt::If(If {
            cond,
            then,
            else_ifs,
            r#else,
        }))
    }

    fn r#for(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::For)?;
        let var = self.must("variable name", |s| s.ident())?;
        self.must_token(Token::In)?;
        let iter = self.must("iterator", |s| {
            let expr = s.expr()?;
            Some(if s.take_if_eq(Token::DotDot).is_some() {
                let start = expr;
                let end = s.must("range end value", |s| s.expr())?;
                let span = start.1.union(end.1);
                (
                    Expr::Range(Range {
                        start: Box::new(start),
                        end: Box::new(end),
                    }),
                    span,
                )
            } else {
                expr
            })
        })?;
        let body = self.must("loop body", |s| s.block())?;
        Some(Stmt::For(For { var, iter, body }))
    }

    fn r#while(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::While)?;
        let cond = self.must("condition", |s| s.expr())?;
        let body = self.must("loop body", |s| s.block())?;
        Some(Stmt::While(While { cond, body }))
    }

    fn r#loop(&mut self) -> Option<Stmt<'src>> {
        self.take_if_eq(Token::Loop)?;
        let body = self.must("loop body", |s| s.block())?;
        Some(Stmt::Loop(Loop { body }))
    }

    fn assign(&mut self) -> Option<Stmt<'src>> {
        // Atomic because both assign and expr start with an expression.
        self.atomic(|s| {
            let target = s.expr()?;
            let op = s.take_if_map(|token| match token {
                Token::Eq => Some(AssignOp::Assign),
                Token::PlusEq => Some(AssignOp::Add),
                Token::MinusEq => Some(AssignOp::Sub),
                Token::StarEq => Some(AssignOp::Mul),
                Token::SlashEq => Some(AssignOp::Div),
                Token::PercentEq => Some(AssignOp::Mod),
                _ => None,
            })?;
            let value = s.must("value", |s| s.expr())?;
            Some(Stmt::Assign(Assign { target, op, value }))
        })
    }

    fn stmt(&mut self) -> Option<Stmt<'src>> {
        self.import()
            .or_else(|| self.func())
            .or_else(|| self.r#struct())
            .or_else(|| self.r#enum())
            .or_else(|| self.var_decl())
            .or_else(|| self.r#if())
            .or_else(|| self.r#for())
            .or_else(|| self.r#while())
            .or_else(|| self.r#loop())
            .or_else(|| self.assign())
            .or_else(|| self.expr().map(Stmt::Expr))
            .or_else(|| {
                let (_, start_span) = self.take_if_eq(Token::Return)?;
                let value = self.atomic(|s| s.expr());
                let end_span = self.src[self.end].1;
                let span = start_span.union(end_span);
                Some(Stmt::Return((value, span)))
            })
            .or_else(|| {
                let (_, span) = self.take_if_eq(Token::Continue)?;
                Some(Stmt::Continue(span))
            })
            .or_else(|| {
                let (_, span) = self.take_if_eq(Token::Break)?;
                Some(Stmt::Break(span))
            })
            .and_then(|stmt| {
                if self
                    .src
                    .get(self.end)
                    .is_some_and(|(token, _)| *token != Token::RBrace)
                {
                    self.take_if_eq(Token::Newline)?;
                }
                Some(stmt)
            })
    }

    fn call(&mut self) -> Option<SpannedExpr<'src>> {
        self.atomic_spanned(|s| {
            let name = s.ident()?;
            let args = s.delimited_by(PARENS, |s| s.separated_by_comma(Parser::expr))?;
            Some(Expr::Call(Call { name, args }))
        })
    }

    // TODO: make specifying more than one type instead of [<type>] report a nice error message
    fn list(&mut self) -> Option<SpannedExpr<'src>> {
        self.atomic_spanned(|s| {
            s.delimited_by(BRACKETS, |s| {
                let elements = s.separated_by_comma(Parser::expr);
                if elements.is_empty()
                    && let Some(element_type) = s.delimited_by(ANGLES, Parser::r#type).flatten()
                {
                    Expr::EmptyList(EmptyList { element_type })
                } else {
                    Expr::List(List { elements })
                }
            })
        })
    }

    // TODO: make specifying only one type instead of {<key, value>} report a nice error message
    fn map_(&mut self) -> Option<SpannedExpr<'src>> {
        self.atomic_spanned(|s| {
            s.delimited_by(BRACES, |s| {
                let pairs = s.separated_by_comma(|s| {
                    let key = s.expr()?;
                    let _ = s.take_if_eq(Token::Colon);
                    let value = s.expr()?;
                    Some(Pair { key, value })
                });
                if pairs.is_empty()
                    && let Some((key_type, value_type)) = s
                        .delimited_by(ANGLES, |s| {
                            let key = s.r#type()?;
                            let _ = s.take_if(|t| t == Token::Comma || t == Token::Colon);
                            let value = s.r#type()?;
                            Some((key, value))
                        })
                        .flatten()
                {
                    Expr::EmptyMap(EmptyMap {
                        key_type,
                        value_type,
                    })
                } else {
                    Expr::Map(Map { pairs })
                }
            })
        })
    }

    fn struct_literal(&mut self) -> Option<SpannedExpr<'src>> {
        self.atomic_spanned(|s| {
            let name = s.ident()?;
            s.take_if(|t| t == Token::Colon || t == Token::ColonColon)?;
            let fields = s.delimited_by(BRACES, |s| {
                s.separated_by_comma(|s| {
                    let name = s.ident()?;
                    s.take_if_eq(Token::Colon)?;
                    let value = s.expr()?;
                    Some(FieldLiteral { name, value })
                })
            })?;
            Some(Expr::StructLiteral(StructLiteral { name, fields }))
        })
    }

    fn variant_literal(&mut self) -> Option<SpannedExpr<'src>> {
        self.atomic_spanned(|s| {
            let enum_name = s.ident()?;
            s.take_if_eq(Token::ColonColon)?;
            let name = s.ident()?;
            let payload = s.delimited_by(PARENS, Parser::expr).flatten().map(Box::new);
            Some(Expr::VariantLiteral(VariantLiteral {
                enum_name,
                name,
                payload,
            }))
        })
    }

    fn format_string(&mut self) -> Option<SpannedExpr<'src>> {
        self.spanned(|s| {
            let text = s.take_if_map(|token| match token {
                Token::StringStart(text) => Some(text),
                _ => None,
            })?;
            let expr = s.must("value", |s| s.expr())?;
            let mut segments = vec![(text, expr)];
            while let Some(text) = s.take_if_map(|token| match token {
                Token::StringMiddle(text) => Some(text),
                _ => None,
            }) {
                let expr = s.must("value", |s| s.expr())?;
                segments.push((text, expr));
            }
            let end_text = s.must("format string end", |s| {
                s.take_if_map(|token| match token {
                    Token::StringEnd(text) => Some(text),
                    _ => None,
                })
            })?;
            Some(Expr::FormatString(FormatString { segments, end_text }))
        })
    }

    fn atom(&mut self) -> Option<SpannedExpr<'src>> {
        self.call()
            .or_else(|| self.list())
            .or_else(|| self.map_())
            .or_else(|| self.struct_literal())
            .or_else(|| self.variant_literal())
            .or_else(|| self.delimited_by(PARENS, Parser::expr).flatten())
            .or_else(|| self.format_string())
            .or_else(|| {
                self.take_if_map(|token| {
                    Some(match token {
                        Token::Ident(name) => Expr::Var(name),
                        Token::Int(int) => Expr::Int(int),
                        Token::Bool(bool) => Expr::Bool(bool),
                        Token::Char(char) => Expr::Char(char),
                        Token::String(string) => Expr::String(string),
                        _ => return None,
                    })
                })
            })
    }

    fn index_or_member_or_atom(&mut self) -> Option<SpannedExpr<'src>> {
        let mut left = self.atom()?;
        loop {
            if let Some((index, _)) = self.atomic_spanned(|s| {
                s.delimited_by(BRACKETS, |s| {
                    let expr = s.expr()?;
                    if s.take_if_eq(Token::DotDot).is_none() {
                        return Some(expr);
                    }
                    let end = s.must("range end value", |s| s.expr())?;
                    let span = expr.1.union(end.1);
                    Some((
                        Expr::Range(Range {
                            start: Box::new(expr),
                            end: Box::new(end),
                        }),
                        span,
                    ))
                })
                .flatten()
            }) {
                let span = left.1.union(index.1);
                left = (
                    Expr::Index(Index {
                        expr: Box::new(left),
                        index: Box::new(index),
                    }),
                    span,
                );
                continue;
            }
            if let Some((member, _)) =
                self.atomic_spanned(|s| s.take_if_eq(Token::Dot).and_then(|_| s.ident()))
            {
                let span = left.1.union(member.1);
                left = (
                    Expr::Member(Member {
                        expr: Box::new(left),
                        member,
                    }),
                    span,
                );
                continue;
            }
            break;
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<SpannedExpr<'src>> {
        self.atomic_spanned(|s| {
            let op = s.take_if_map(|t| match t {
                Token::Minus => Some(UnaryOp::Neg),
                Token::Not => Some(UnaryOp::Not),
                _ => None,
            })?;
            let expr = Box::new(s.must("value", |s| s.index_or_member_or_atom())?);
            Some(Expr::Unary(Unary { op, expr }))
        })
    }

    fn unary_or_index_or_member_or_atom(&mut self) -> Option<SpannedExpr<'src>> {
        self.unary().or_else(|| self.index_or_member_or_atom())
    }

    fn binary_or_unary_or_index_or_member_or_atom(&mut self) -> Option<SpannedExpr<'src>> {
        macro_rules! parser {
            ($name:ident, match { $($tok:ident => $op:ident),*$(,)? }, $prev:expr$(,)?) => {
                fn $name<'src>(s: &mut Parser<'_, 'src>) -> Option<SpannedExpr<'src>> {
                    s.parse_binary(
                        |op| match op {
                            $(Token::$tok => Some(BinaryOp::$op),)*
                            _ => None,
                        },
                        $prev,
                    )
                }
            };
        }
        parser!(
            product,
            match { Star => Mul, Slash => Div, Percent => Mod },
            |s| s.unary_or_index_or_member_or_atom(),
        );
        parser!(sum, match { Plus => Add, Minus => Sub }, product);
        parser!(ordering, match { LAngle => Lt, LAngleEq => Leq, RAngle => Gt, RAngleEq => Geq }, sum);
        parser!(equal, match { EqEq => Eq, BangEq => NotEq }, ordering);
        parser!(and, match { And => And }, equal);
        parser!(or, match { Or => Or }, and);
        or(self)
    }

    fn expr(&mut self) -> Option<SpannedExpr<'src>> {
        self.binary_or_unary_or_index_or_member_or_atom()
    }
}

pub fn parse<'src>(source: &[SpannedToken<'src>]) -> (Ast<'src>, Vec<ParseError<'src>>) {
    let source = source
        .iter()
        .copied()
        .filter(|(token, _)| !matches!(token, Token::Comment(_)))
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    let mut parser = Parser {
        src: &source,
        errors: &mut errors,
        end: 0,
    };
    // This check allows us to assume that source[0] is valid.
    let stmts = parser.repeated(Parser::stmt);
    let end = parser.end;
    if end < source.len() && errors.is_empty() {
        errors.push((
            format!("Expected function, struct, variable declaration, import or expression"),
            source[end].1,
        ));
    }
    let ast = Ast { stmts };
    (ast, errors)
}
