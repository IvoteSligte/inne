use super::*;
use indentify::*;

/// Wrapper for implementing [std::fmt::Display] for a type that can otherwise not implement it.
pub(crate) struct Display<T>(pub(crate) T);

// It would be easy to call Display on Type by accident in a type-checking error message
// if it was implemented directly, so we use a wrapper to prevent that.
impl std::fmt::Display for Display<&Type> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match &self.0 {
            Type::Nothing => "nothing",
            Type::Int => "int",
            Type::Bool => "bool",
            Type::String => "string",
            Type::Char => "char",
            Type::Range => "range",
            Type::List(element) => return write!(f, "List<{}>", Display(element.as_ref())),
            Type::Map { key, value } => {
                return write!(
                    f,
                    "Map<{}, {}>",
                    Display(key.as_ref()),
                    Display(value.as_ref())
                );
            }
            Type::Named { id, generics } => {
                write!(f, "${id}")?;
                if !generics.is_empty() {
                    f.write_str("<")?;
                    write_comma_separated(f, generics.iter().map(Display))?;
                    f.write_str(">")?;
                }
                return Ok(());
            }
            Type::Generic(id) => return write!(f, "#{id}"),
        })
    }
}

impl std::fmt::Display for Hir<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: Not sure why it doesn't work without the dyn conversion.
        let f: &mut dyn std::fmt::Write = &mut IndentWriter::new("    ", f);
        writeln!(f, "HIR:")?;
        writeln!(f, "vars: {{{INDENT}")?;
        for (i, VarInfo { name, span }) in self.vars.iter().enumerate() {
            writeln!(f, "_{i}: var {name} at {span};")?;
        }
        writeln!(f, "{DEDENT}}}")?;

        for Func {
            id,
            name: (name, _),
            generics,
            params,
            return_type,
            body,
        } in &self.funcs
        {
            write!(f, "func _{} {name}", id)?;
            if !generics.is_empty() {
                f.write_str("<")?;
                for (i, generic) in generics.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", generic.0)?;
                }
                f.write_str(">")?;
            }
            f.write_str("(")?;
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(
                    f,
                    "_{} {}: {}",
                    param.var_id,
                    param.name.0,
                    Display(&param.r#type.0)
                )?;
            }
            write!(f, ") ")?;
            if let Some((return_type, _)) = return_type {
                write!(f, "-> {} ", Display(return_type))?;
            }
            writeln!(f, "{{{INDENT}")?;
            for stmt in body {
                writeln!(f, "{stmt}")?;
            }
            writeln!(f, "{DEDENT}}}")?;
        }
        for stmt in &self.stmts {
            writeln!(f, "{stmt}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Stmt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Stmt::VarDecl(VarDecl {
                var_id,
                name: (name, _),
                value: (value, _),
            }) => {
                write!(f, "var _{var_id} {name} = {value};")?;
            }
            Stmt::Expr((expr, _)) => write!(f, "{expr};")?,
            Stmt::Assign(Assign {
                op: (op, _),
                target: (target, _),
                value: (value, _),
            }) => {
                write!(f, "{target} {op} {value};")?;
            }
            Stmt::If(If { choices, r#else }) => {
                writeln!(f, "if {} {{{INDENT}", choices[0].cond.0)?;
                for stmt in &choices[0].then {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
                for choice in &choices[1..] {
                    writeln!(f, " else if {} {{{INDENT}", choice.cond.0)?;
                    for stmt in &choice.then {
                        writeln!(f, "{stmt}")?;
                    }
                    write!(f, "{DEDENT}}}")?;
                }
                if let Some(r#else) = r#else {
                    writeln!(f, " else {{{INDENT}")?;
                    for stmt in r#else {
                        writeln!(f, "{stmt}")?;
                    }
                    write!(f, "{DEDENT}}}")?;
                }
            }
            Stmt::For(For {
                var_id,
                var: (var, _),
                iter: (iter, _),
                body,
            }) => {
                writeln!(f, "for _{var_id} {var} in {iter} {{{INDENT}")?;
                for stmt in body {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
            }
            Stmt::While(While {
                cond: (cond, _),
                body,
            }) => {
                writeln!(f, "while {cond} {{{INDENT}")?;
                for stmt in body {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
            }
            Stmt::Loop(Loop { body }) => {
                writeln!(f, "loop {{{INDENT}")?;
                for stmt in body {
                    writeln!(f, "{stmt}")?;
                }
                write!(f, "{DEDENT}}}")?;
            }
            Stmt::Break(_) => write!(f, "break;")?,
            Stmt::Continue(_) => write!(f, "continue;")?,
            Stmt::Return((value, _)) => {
                f.write_str("return")?;
                if let Some((value, _)) = value {
                    write!(f, " {value}")?;
                }
                f.write_str(";")?;
            }
        }
        Ok(())
    }
}

impl<'a> std::fmt::Display for Expr<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Binary(Binary { op, left, right }) => {
                write!(f, "({} {} {})", left.0, op.0, right.0)?
            }
            Expr::Unary(Unary { op, expr }) => write!(f, "({}{})", op.0, expr.0)?,
            Expr::Call(Call { name, args, .. }) => {
                write!(f, "{}(", name.0)?;
                write_comma_separated(f, args.iter().map(|(arg, _)| arg))?;
                f.write_str(")")?;
            }
            Expr::EmptyList(EmptyList { element_type }) => {
                f.write_str("[<")?;
                Display(&element_type.0).fmt(f)?;
                f.write_str(">]")?;
            }
            Expr::List(List { elements }) => {
                f.write_str("[")?;
                write_comma_separated(f, elements.iter().map(|(elem, _)| elem))?;
                f.write_str("]")?;
            }
            Expr::EmptyMap(EmptyMap {
                key_type,
                value_type,
            }) => {
                f.write_str("{<")?;
                Display(&key_type.0).fmt(f)?;
                f.write_str(", ")?;
                Display(&value_type.0).fmt(f)?;
                f.write_str(">}")?;
            }
            Expr::Map(Map { pairs }) => {
                f.write_str("{")?;
                write_comma_separated(f, pairs.iter())?;
                f.write_str("}")?;
            }
            Expr::Index(Index { expr, index }) => {
                f.write_str("(")?;
                expr.0.fmt(f)?;
                f.write_str("[")?;
                index.0.fmt(f)?;
                f.write_str("]")?;
                f.write_str(")")?;
            }
            Expr::Range(Range { start, end }) => write!(f, "{}..{}", start.0, end.0)?,
            Expr::Var(var_id) => write!(f, "_{var_id}")?,
            Expr::Int(int) => write!(f, "{int}")?,
            Expr::Bool(bool) => write!(f, "{bool}")?,
            Expr::String(string) => write!(f, "{string:?}")?,
            Expr::Char(char) => write!(f, "{char:?}")?,
            Expr::StructLiteral(StructLiteral {
                type_id,
                name,
                fields,
            }) => {
                write!(f, "_{type_id} {} ", name.0)?;
                f.write_str("{")?;
                write_comma_separated(f, fields.iter())?;
                f.write_str("}")?;
            }
            Expr::VariantLiteral(VariantLiteral {
                type_id,
                variant_index,
                enum_name,
                name,
                payload,
            }) => {
                write!(
                    f,
                    "_{type_id} {} :: _{variant_index} {}",
                    enum_name.0, name.0
                )?;
                if let Some(payload) = payload {
                    write!(f, "({})", payload.0)?;
                }
            }
            Expr::Member(Member { expr, member, .. }) => {
                write!(f, "({}).{}", expr.0, member.0)?;
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for Pair<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.key.0.fmt(f)?;
        f.write_str(": ")?;
        self.value.0.fmt(f)
    }
}

impl std::fmt::Display for FieldLiteral<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.0.fmt(f)?;
        f.write_str(": ")?;
        self.value.0.fmt(f)
    }
}
