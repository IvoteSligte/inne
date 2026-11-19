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

macro_rules! any {
    ($first:expr, $($expr:expr),*$(,)?) => {
        $first $(.or_else(|| $expr))*
    }
}

pub enum Id {
    Var(VarId),
    Func(FuncId),
    TypeDef(TypeId),
    Field(usize),
    Variant(usize),
    Generic(usize),
    Builtin,
    /// Used for variants and members, since those symbols are only resolved during type checking.
    Unresolved,
}

pub fn func_find<'src>(func: &Func<'src>, target: Span<'src>) -> Option<Id> {
    let Func {
        id,
        name,
        generics,
        params,
        return_type,
        body,
    } = func;
    any!(
        (name.1 == target).then_some(Id::Func(*id)),
        generics_find(generics, target),
        params.iter().find_map(
            |Param {
                 var_id,
                 name,
                 r#type,
             }| {
                any!(
                    (name.1 == target).then_some(Id::Var(*var_id)),
                    type_find(r#type, target)
                )
            }
        ),
        return_type
            .as_ref()
            .and_then(|r#type| type_find(r#type, target)),
        stmts_find(body, target)
    )
}

pub fn type_def_find<'src>(type_def: &TypeDef<'src>, target: Span<'src>) -> Option<Id> {
    match type_def {
        TypeDef::Struct(Struct {
            type_id,
            name,
            generics,
            fields,
        }) => {
            any!(
                (name.1 == target).then_some(Id::TypeDef(*type_id)),
                generics_find(generics, target),
                fields
                    .iter()
                    .enumerate()
                    .find_map(|(index, Field { name, r#type })| {
                        any!(
                            (name.1 == target).then_some(Id::Field(index)),
                            type_find(r#type, target)
                        )
                    })
            )
        }
        TypeDef::Enum(Enum {
            type_id,
            name,
            generics,
            variants,
        }) => {
            any!(
                (name.1 == target).then_some(Id::TypeDef(*type_id)),
                generics_find(generics, target),
                variants
                    .iter()
                    .enumerate()
                    .find_map(|(index, Variant { name, payload })| {
                        any!(
                            (name.1 == target).then_some(Id::Variant(index)),
                            payload
                                .as_ref()
                                .and_then(|payload| type_find(payload, target))
                        )
                    })
            )
        }
    }
}

pub fn type_find<'src>((r#type, span): &Spanned<'src, Type>, target: Span<'src>) -> Option<Id> {
    type_find_inner((r#type, *span), target)
}

// TODO: accurately determine and match list element, map key/value and named type generic spans
pub fn type_find_inner<'src>(
    (r#type, span): Spanned<'src, &Type>,
    target: Span<'src>,
) -> Option<Id> {
    match r#type {
        Type::Nothing | Type::Int | Type::Bool | Type::Char | Type::UnknownGeneric => {
            (span == target).then_some(Id::Builtin)
        }
        Type::Range | Type::Ref(_) => unreachable!(),
        Type::List(element) => {
            let Span { start, end, .. } = span;
            let name_span = Span {
                end: start + 4,
                ..span
            };
            // This is an estimate that assumes there is no whitespace between tokens in the type.
            let element_span = Span {
                start: start + 5,
                end: end - 1,
                ..span
            };
            any!(
                (name_span == target).then_some(Id::Builtin),
                type_find_inner((element, element_span), target)
            )
        }
        Type::Map { key: _, value: _ } => {
            let Span { start, .. } = span;
            let name_span = Span {
                end: start + 3,
                ..span
            };
            (name_span == target).then_some(Id::Builtin)
            // The key and value spans cannot be determined, so they cannot be matched.
        }
        Type::Named { id, generics } => {
            (generics.is_empty() && span == target).then_some(Id::TypeDef(*id))
            // The spans of generics cannot be determined.
        }
        Type::Generic(generic_id) => (span == target).then_some(Id::Generic(generic_id.get())),
    }
}

pub fn generics_find<'src>(generics: &[Name<'src>], target: Span<'src>) -> Option<Id> {
    generics
        .iter()
        .enumerate()
        .find_map(|(index, generic)| (generic.1 == target).then_some(Id::Generic(index)))
}

pub fn stmts_find<'src>(stmts: &[Stmt<'src>], target: Span<'src>) -> Option<Id> {
    stmts.iter().find_map(|stmt| stmt_find(stmt, target))
}

pub fn stmt_find<'src>(stmt: &Stmt<'src>, target: Span<'src>) -> Option<Id> {
    match stmt {
        Stmt::VarDecl(var_decl) => (var_decl.name.1 == target).then_some(Id::Var(var_decl.var_id)),
        Stmt::Expr(expr) => expr_find(expr, target),
        Stmt::Assign(Assign {
            op: _,
            target: assign_target,
            value,
        }) => any!(expr_find(assign_target, target), expr_find(value, target)),
        Stmt::If(If { choices, r#else }) => any!(
            choices.iter().find_map(|choice| {
                any!(
                    expr_find(&choice.cond, target),
                    stmts_find(&choice.then, target)
                )
            }),
            r#else
                .as_ref()
                .and_then(|r#else| stmts_find(r#else, target))
        ),
        Stmt::Match(Match { expr, arms }) => any!(
            expr_find(expr, target),
            arms.iter().find_map(|arm| {
                pattern_find(&arm.pattern, target).or_else(|| stmts_find(&arm.body, target))
            })
        ),
        Stmt::For(For {
            var_id,
            var,
            iter,
            body,
        }) => any!(
            (var.1 == target).then_some(Id::Var(*var_id)),
            expr_find(iter, target),
            stmts_find(&body, target)
        ),
        Stmt::While(While { cond, body }) => {
            any!(expr_find(cond, target), stmts_find(body, target))
        }
        Stmt::Loop(Loop { body }) => stmts_find(body, target),
        Stmt::Break(_) | Stmt::Continue(_) => None,
        Stmt::Return((expr, _)) => expr.as_ref().and_then(|expr| expr_find(expr, target)),
    }
}

pub fn expr_find<'src>((expr, span): &SpannedExpr<'src>, target: Span<'src>) -> Option<Id> {
    expr_find_inner((expr, *span), target)
}

pub fn expr_find_inner<'src>(
    (expr, span): Spanned<'src, &Expr<'src>>,
    target: Span<'src>,
) -> Option<Id> {
    match expr {
        Expr::Binary(Binary { op: _, left, right }) => {
            any!(expr_find(left, target), expr_find(right, target))
        }
        Expr::Unary(Unary { op: _, expr }) => expr_find(expr, target),
        Expr::Call(Call {
            name,
            func_id,
            args,
        }) => {
            any!(
                (name.1 == target).then_some(Id::Func(*func_id)),
                args.iter().find_map(|arg| expr_find(arg, target))
            )
        }
        Expr::EmptyList(EmptyList { element_type }) => type_find(element_type, target),
        Expr::List(List { elements }) => elements
            .iter()
            .find_map(|element| expr_find(element, target)),
        Expr::EmptyMap(EmptyMap {
            key_type,
            value_type,
        }) => {
            any!(type_find(key_type, target), type_find(value_type, target),)
        }
        Expr::Map(Map { pairs }) => pairs.iter().find_map(|Pair { key, value }| {
            any!(expr_find(key, target), expr_find(value, target),)
        }),
        Expr::StructLiteral(StructLiteral {
            type_id,
            name,
            fields,
        }) => any!(
            (name.1 == target).then_some(Id::TypeDef(*type_id)),
            fields
                .iter()
                .enumerate()
                .find_map(|(index, FieldLiteral { name, value })| any!(
                    (name.1 == target).then_some(Id::Field(index)),
                    expr_find(value, target)
                ))
        ),
        Expr::VariantLiteral(VariantLiteral {
            type_id,
            variant_index,
            enum_name,
            name,
            payload,
        }) => {
            any!(
                (enum_name.1 == target).then_some(Id::TypeDef(*type_id)),
                (name.1 == target).then_some(Id::Variant(*variant_index)),
                payload
                    .as_ref()
                    .and_then(|payload| expr_find(payload, target))
            )
        }
        Expr::Index(Index { expr, index }) => {
            any!(expr_find(expr, target), expr_find(index, target))
        }
        Expr::Member(Member {
            type_id: _,     // Unresolved in HIR
            field_index: _, // Unresolved in HIR
            expr,
            member,
        }) => {
            any!(
                expr_find(expr, target),
                (member.1 == target).then_some(Id::Unresolved)
            )
        }
        Expr::Range(Range { start, end }) => {
            any!(expr_find(start, target), expr_find(end, target))
        }
        Expr::Var(var_id) => (span == target).then_some(Id::Var(*var_id)),
        Expr::VarRef(var_id) => (span == target).then_some(Id::Var(*var_id)),
        Expr::Int(_) | Expr::Bool(_) | Expr::Char(_) | Expr::String(_) => None,
        Expr::FormatString(FormatString {
            segments,
            end_text: _,
        }) => segments
            .iter()
            .find_map(|(_, expr)| expr_find(expr, target)),
        Expr::Deref(expr) => expr_find_inner((expr, span), target),
        Expr::Copy(expr) => expr_find_inner((expr, span), target),
    }
}

pub fn pattern_find<'src>(pattern: &Pattern<'src>, target: Span<'src>) -> Option<Id> {
    match pattern {
        Pattern::Variant {
            variant_index: _,
            name,
            payload,
        } => any!(
            (name.1 == target).then_some(Id::Unresolved),
            payload.and_then(|(_, payload)| (payload.1 == target).then_some(Id::Unresolved))
        ),
    }
}
