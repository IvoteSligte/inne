use std::fmt::{Result, Write};

use indentify::*;
use inne::{Spanned, ast::*, lex, parse};

pub trait Format {
    fn fmt(&self, f: &mut impl Write) -> Result;
}

impl<T: Format> Format for &T {
    fn fmt(&self, f: &mut impl Write) -> Result {
        (**self).fmt(f)
    }
}

impl<T: Format> Format for Spanned<T> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.0.fmt(f)
    }
}

impl Format for &str {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str(self)
    }
}

impl Format for bool {
    fn fmt(&self, f: &mut impl Write) -> Result {
        write!(f, "{self}")
    }
}

macro_rules! delegate {
    ($self:ident, $f:ident, $($variant:ident),*; $($variant2:ident($value:ident) => $expr:expr),*) => {
        match $self {
            $($variant(item) => item.fmt($f),)*
            $($variant2($value) => $expr,)*
        }
    }
}

impl Format for Ast<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        for stmt in &self.stmts {
            stmt.fmt(f)?;
            f.write_str("\n\n")?;
        }
        Ok(())
    }
}

impl Format for Stmt<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        use Stmt::*;
        delegate!(self, f,
            Func,
            Struct,
            VarDecl,
            Expr,
            Assign,
            If,
            For,
            While,
            Loop;
            Break(_span) => write!(f, "break;"),
            Continue(_span) => write!(f, "continue;"),
            Return(value) => {
                match &value.0 {
                    Some((value, _)) => {
                        f.write_str("return ")?;
                        value.fmt(f)?;
                        f.write_str(";")
                    }
                    None => f.write_str("return;")
                }
            }
        )
    }
}

impl Format for Block<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        if self.is_empty() {
            return f.write_str("{}");
        }
        writeln!(f, "{{{INDENT}")?;
        for stmt in self {
            stmt.fmt(f)?;
            f.write_str("\n")?;
        }
        write!(f, "{DEDENT}}}")
    }
}

impl Format for Type<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str(self.name.0)?;
        if !self.generics.is_empty() {
            f.write_str("<")?;
            write_comma_separated(f, &self.generics)?;
            f.write_str(">")?;
        }
        Ok(())
    }
}

impl Format for Func<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("func ")?;
        f.write_str(self.name.0)?;
        f.write_str("(")?;
        write_comma_separated(f, &self.params)?;
        f.write_str(") ")?;
        if let Some(r#type) = self.return_type.as_ref() {
            f.write_str("-> ")?;
            r#type.fmt(f)?;
        }
        self.body.fmt(f)
    }
}

impl Format for Param<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.name.fmt(f)?;
        f.write_str(": ")?;
        self.r#type.fmt(f)
    }
}

impl Format for Struct<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("struct ")?;
        f.write_str(self.name.0)?;
        if self.fields.is_empty() {
            f.write_str("{}")
        } else {
            writeln!(f, " {{{INDENT}")?;
            for field in &self.fields {
                field.fmt(f)?;
                writeln!(f, ",")?;
            }
            write!(f, "{DEDENT}}}")
        }
    }
}

impl Format for Field<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.name.fmt(f)?;
        f.write_str(": ")?;
        self.r#type.fmt(f)
    }
}

impl Format for VarDecl<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("var ")?;
        self.name.fmt(f)?;
        f.write_str(" = ")?;
        self.value.fmt(f)?;
        f.write_str(";")
    }
}

impl Format for Expr<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
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
            Index,
            Range,
            Var,
            Int,
            Bool,
            String;
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
    fn fmt(&self, f: &mut impl Write) -> Result {
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
                f.write_str("(")?;
            }
            self.left.fmt(f)?;
            if left_needs_parens {
                f.write_str(")")?;
            }
        }
        f.write_str(" ")?;
        self.op.0.fmt(f)?;
        f.write_str(" ")?;
        {
            if right_needs_parens {
                f.write_str("(")?;
            }
            self.right.fmt(f)?;
            if right_needs_parens {
                f.write_str(")")?;
            }
        }
        Ok(())
    }
}

impl Format for BinaryOp {
    fn fmt(&self, f: &mut impl Write) -> Result {
        // Delegate to Display implementation.
        write!(f, "{self}")
    }
}

impl Format for Unary<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.op.0.fmt(f)?;
        let needs_parens = matches!(
            &self.expr.0,
            Expr::Binary(_) | Expr::Unary(_) | Expr::Range(_)
        );
        if needs_parens {
            f.write_str("(")?;
        }
        self.expr.fmt(f)?;
        if needs_parens {
            f.write_str(")")?;
        }
        Ok(())
    }
}

impl Format for UnaryOp {
    fn fmt(&self, f: &mut impl Write) -> Result {
        // Delegate to Display implementation.
        write!(f, "{self}")
    }
}

impl Format for Call<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.name.fmt(f)?;
        f.write_str("(")?;
        write_comma_separated(f, &self.args)?;
        f.write_str(")")
    }
}

impl Format for EmptyList<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("[<")?;
        self.element_type.fmt(f)?;
        f.write_str(">]")
    }
}

impl Format for List<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("[")?;
        write_comma_separated(f, &self.elements)?;
        f.write_str("]")
    }
}

impl Format for EmptyMap<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("{<")?;
        self.key_type.fmt(f)?;
        f.write_str(", ")?;
        self.value_type.fmt(f)?;
        f.write_str(">}")
    }
}

impl Format for Map<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("{{")?;
        write_comma_separated(f, &self.pairs)?;
        f.write_str("}}")
    }
}

impl Format for Pair<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.key.fmt(f)?;
        f.write_str(": ")?;
        self.value.fmt(f)
    }
}

impl Format for StructLiteral<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.name.fmt(f)?;
        f.write_str(" {{ ")?;
        write_comma_separated(f, &self.fields)?;
        f.write_str(" }}")
    }
}

impl Format for FieldLiteral<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.name.fmt(f)?;
        f.write_str(": ")?;
        self.value.fmt(f)
    }
}

impl Format for Index<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        let needs_parens = matches!(
            self.expr.0,
            Expr::Binary(_) | Expr::Unary(_) | Expr::Range(_)
        );
        if needs_parens {
            f.write_str("(")?;
        }
        self.expr.fmt(f)?;
        if needs_parens {
            f.write_str(")")?;
        }
        f.write_str("[")?;
        self.index.fmt(f)?;
        f.write_str("]")
    }
}

impl Format for Range<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        let start_needs_parens = matches!(self.start.0, Expr::Range(_));
        let end_needs_parens = matches!(self.start.0, Expr::Range(_));

        {
            if start_needs_parens {
                f.write_str("(")?;
            }
            self.start.fmt(f)?;
            if start_needs_parens {
                f.write_str(")")?;
            }
        }
        f.write_str("..")?;
        {
            if end_needs_parens {
                f.write_str("(")?;
            }
            self.end.fmt(f)?;
            if end_needs_parens {
                f.write_str(")")?;
            }
        }
        Ok(())
    }
}

impl Format for Assign<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        self.target.fmt(f)?;
        f.write_str(" ")?;
        self.op.fmt(f)?;
        f.write_str(" ")?;
        self.value.fmt(f)?;
        f.write_str(";")
    }
}

impl Format for AssignOp {
    fn fmt(&self, f: &mut impl Write) -> Result {
        // Delegates to Display implementation.
        write!(f, "{self}")
    }
}

impl Format for If<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("if ")?;
        self.cond.fmt(f)?;
        f.write_str(" ")?;
        self.then.fmt(f)?;
        for (cond, then) in &self.else_ifs {
            f.write_str(" else if ")?;
            cond.fmt(f)?;
            f.write_str(" ")?;
            then.fmt(f)?;
        }
        if let Some(r#else) = &self.r#else {
            f.write_str(" else ")?;
            r#else.fmt(f)?;
        }
        Ok(())
    }
}

impl Format for For<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("for ")?;
        self.var.fmt(f)?;
        f.write_str(" in ")?;
        self.iter.fmt(f)?;
        f.write_str(" ")?;
        self.body.fmt(f)?;
        Ok(())
    }
}

impl Format for While<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("while ")?;
        self.cond.fmt(f)?;
        f.write_str(" ")?;
        self.body.fmt(f)?;
        Ok(())
    }
}

impl Format for Loop<'_> {
    fn fmt(&self, f: &mut impl Write) -> Result {
        f.write_str("loop ")?;
        self.body.fmt(f)?;
        Ok(())
    }
}

/// Returns `None` on parsing failure and `Some(Err(_))` on write failure.
pub fn format(source: &str, dest: impl Write) -> Option<Result> {
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
    // TODO: re-insert comments (retokenize the output and match new tokens to old tokens to find out where to place comments)
    let mut dest = IndentWriter::new("    ", dest);
    Some(ast.fmt(&mut dest))
}

fn write_comma_separated(
    f: &mut impl Write,
    items: impl IntoIterator<Item = impl Format>,
) -> Result {
    Ok(for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        item.fmt(f)?;
    })
}
