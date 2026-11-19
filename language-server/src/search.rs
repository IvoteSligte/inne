use inne::{Span, Spanned, hir::*, parser::SpannedToken};

pub fn tokens_find<'src>(
    tokens: &[SpannedToken<'src>],
    offset: usize,
) -> Option<SpannedToken<'src>> {
    for token @ (_, span) in tokens {
        if span.contains(offset) {
            return Some(*token);
        }
    }
    None
}

pub fn extract_ids<'src>(hir: &Hir<'src>) -> Vec<(Span<'src>, Id)> {
    let mut ids = Vec::new();
    stmts_find(&hir.stmts, &mut |span, id| ids.push((span, id)));
    hir.funcs
        .iter()
        .for_each(|func| func_find(func, &mut |span, id| ids.push((span, id))));
    hir.types
        .iter()
        .for_each(|r#type| type_def_find(r#type, &mut |span, id| ids.push((span, id))));
    ids
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Id {
    Var(VarId),
    Func(FuncId),
    TypeDef(TypeId),
    Field(usize),
    Variant(usize),
    Generic(usize),
    Nothing,
    Int,
    Bool,
    Char,
    String,
    UnknownGeneric,
    /// Used for variants and members, since those symbols are only resolved during type checking.
    Unresolved,
}

pub fn func_find<'src>(func: &Func<'src>, f: &mut impl FnMut(Span<'src>, Id)) {
    let Func {
        id,
        name,
        generics,
        params,
        return_type,
        body,
    } = func;
    f(name.1, Id::Func(*id));
    generics_find(generics, f);
    params.iter().for_each(
        |Param {
             var_id,
             name,
             r#type,
         }| {
            f(name.1, Id::Var(*var_id));
            type_find(r#type, f);
        },
    );
    return_type.as_ref().inspect(|r#type| type_find(r#type, f));
    stmts_find(body, f);
}

pub fn type_def_find<'src>(type_def: &TypeDef<'src>, f: &mut impl FnMut(Span<'src>, Id)) {
    match type_def {
        TypeDef::Struct(Struct {
            type_id,
            name,
            generics,
            fields,
        }) => {
            f(name.1, Id::TypeDef(*type_id));
            generics_find(generics, f);
            fields
                .iter()
                .enumerate()
                .for_each(|(index, Field { name, r#type })| {
                    f(name.1, Id::Field(index));
                    type_find(r#type, f);
                });
        }
        TypeDef::Enum(Enum {
            type_id,
            name,
            generics,
            variants,
        }) => {
            f(name.1, Id::TypeDef(*type_id));
            generics_find(generics, f);
            variants
                .iter()
                .enumerate()
                .for_each(|(index, Variant { name, payload })| {
                    f(name.1, Id::Variant(index));
                    payload.as_ref().inspect(|payload| type_find(payload, f));
                });
        }
    }
}

pub fn type_find<'src>((r#type, span): &Spanned<'src, Type>, f: &mut impl FnMut(Span<'src>, Id)) {
    type_find_inner((r#type, *span), f)
}

// TODO: accurately determine and match list element, map key/value and named type generic spans
pub fn type_find_inner<'src>(
    (r#type, span): Spanned<'src, &Type>,
    f: &mut impl FnMut(Span<'src>, Id),
) {
    match r#type {
        Type::Nothing => f(span, Id::Nothing),
        Type::Int => f(span, Id::Int),
        Type::Bool => f(span, Id::Bool),
        Type::Char => f(span, Id::Char),
        Type::UnknownGeneric => f(span, Id::UnknownGeneric),
        Type::Range | Type::Ref(_) => unreachable!(),
        Type::List(_) => {
            // The component spans cannot be determined.
        }
        Type::Map { key: _, value: _ } => {
            // The component spans cannot be determined.
        }
        Type::Named { id, generics } => {
            if generics.is_empty() {
                f(span, Id::TypeDef(*id))
            }
            // The spans of generics cannot be determined.
        }
        Type::Generic(generic_id) => f(span, Id::Generic(generic_id.get())),
    }
}

pub fn generics_find<'src>(generics: &[Name<'src>], f: &mut impl FnMut(Span<'src>, Id)) {
    generics
        .iter()
        .enumerate()
        .for_each(|(index, generic)| f(generic.1, Id::Generic(index)))
}

pub fn stmts_find<'src>(stmts: &[Stmt<'src>], f: &mut impl FnMut(Span<'src>, Id)) {
    stmts.iter().for_each(|stmt| stmt_find(stmt, f))
}

pub fn stmt_find<'src>(stmt: &Stmt<'src>, f: &mut impl FnMut(Span<'src>, Id)) {
    match stmt {
        Stmt::VarDecl(var_decl) => f(var_decl.name.1, Id::Var(var_decl.var_id)),
        Stmt::Expr(expr) => expr_find(expr, f),
        Stmt::Assign(Assign {
            op: _,
            target: assign_target,
            value,
        }) => {
            expr_find(assign_target, f);
            expr_find(value, f)
        }
        Stmt::If(If { choices, r#else }) => {
            choices.iter().for_each(|choice| {
                expr_find(&choice.cond, f);
                stmts_find(&choice.then, f);
            });
            r#else.as_ref().inspect(|r#else| stmts_find(r#else, f));
        }
        Stmt::Match(Match { expr, arms }) => {
            expr_find(expr, f);
            arms.iter().for_each(|arm| {
                pattern_find(&arm.pattern, f);
                stmts_find(&arm.body, f);
            })
        }
        Stmt::For(For {
            var_id,
            var,
            iter,
            body,
        }) => {
            f(var.1, Id::Var(*var_id));
            expr_find(iter, f);
            stmts_find(&body, f);
        }
        Stmt::While(While { cond, body }) => {
            expr_find(cond, f);
            stmts_find(body, f);
        }
        Stmt::Loop(Loop { body }) => stmts_find(body, f),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Return((expr, _)) => {
            expr.as_ref().inspect(|expr| expr_find(expr, f));
        }
    }
}

pub fn expr_find<'src>((expr, span): &SpannedExpr<'src>, f: &mut impl FnMut(Span<'src>, Id)) {
    expr_find_inner((expr, *span), f)
}

pub fn expr_find_inner<'src>(
    (expr, span): Spanned<'src, &Expr<'src>>,
    f: &mut impl FnMut(Span<'src>, Id),
) {
    match expr {
        Expr::Binary(Binary { op: _, left, right }) => {
            expr_find(left, f);
            expr_find(right, f);
        }
        Expr::Unary(Unary { op: _, expr }) => expr_find(expr, f),
        Expr::Call(Call {
            name,
            func_id,
            args,
        }) => {
            f(name.1, Id::Func(*func_id));
            args.iter().for_each(|arg| expr_find(arg, f));
        }
        Expr::EmptyList(EmptyList { element_type }) => type_find(element_type, f),
        Expr::List(List { elements }) => elements.iter().for_each(|element| expr_find(element, f)),
        Expr::EmptyMap(EmptyMap {
            key_type,
            value_type,
        }) => {
            type_find(key_type, f);
            type_find(value_type, f);
        }
        Expr::Map(Map { pairs }) => pairs.iter().for_each(|Pair { key, value }| {
            expr_find(key, f);
            expr_find(value, f);
        }),
        Expr::StructLiteral(StructLiteral {
            type_id,
            name,
            fields,
        }) => {
            f(name.1, Id::TypeDef(*type_id));
            fields
                .iter()
                .enumerate()
                .for_each(|(index, FieldLiteral { name, value })| {
                    (name.1, Id::Field(index));
                    expr_find(value, f);
                });
        }
        Expr::VariantLiteral(VariantLiteral {
            type_id,
            variant_index,
            enum_name,
            name,
            payload,
        }) => {
            (enum_name.1, Id::TypeDef(*type_id));
            (name.1, Id::Variant(*variant_index));
            payload.as_ref().inspect(|payload| expr_find(payload, f));
        }
        Expr::Index(Index { expr, index }) => {
            expr_find(expr, f);
            expr_find(index, f)
        }
        Expr::Member(Member {
            type_id: _,     // Unresolved in HIR
            field_index: _, // Unresolved in HIR
            expr,
            member,
        }) => {
            expr_find(expr, f);
            f(member.1, Id::Unresolved)
        }
        Expr::Range(Range { start, end }) => {
            expr_find(start, f);
            expr_find(end, f)
        }
        Expr::Var(var_id) => f(span, Id::Var(*var_id)),
        Expr::VarRef(var_id) => f(span, Id::Var(*var_id)),
        Expr::Int(_) | Expr::Bool(_) | Expr::Char(_) | Expr::String(_) => {}
        Expr::FormatString(FormatString {
            segments,
            end_text: _,
        }) => segments.iter().for_each(|(_, expr)| expr_find(expr, f)),
        Expr::Deref(expr) => expr_find_inner((expr, span), f),
        Expr::Copy(expr) => expr_find_inner((expr, span), f),
    }
}

pub fn pattern_find<'src>(pattern: &Pattern<'src>, f: &mut impl FnMut(Span<'src>, Id)) {
    match pattern {
        Pattern::Variant {
            variant_index: _,
            name,
            payload,
        } => {
            (name.1, Id::Unresolved);
            payload.inspect(|(_, payload)| f(payload.1, Id::Unresolved));
        }
    }
}
