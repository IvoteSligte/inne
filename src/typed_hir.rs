use std::{collections::HashMap, ops::Deref};

pub use crate::hir::{
    Assign, AssignOp, Binary, BinaryOp, Block, Call, Choice, EmptyList, EmptyMap, Enum, Expr, For,
    FormatString, Func, FuncId, GenericId, Hir, If, Index, List, Loop, Map, Match, MatchArm,
    Member, Name, Pair, Param, Pattern, Range, SpannedExpr, Stmt, Struct, StructLiteral, Type,
    TypeDef, TypeId, Unary, UnaryOp, VarDecl, VarId, Variant, VariantLiteral, While,
};

use crate::{Error, Errors, Span, Spanned, hir, util::write_comma_separated};

/// Type-checked high-level intermediate representation.
/// Contains type annotations for all function and variable definitions.
pub struct TypedHir<'src> {
    /// Essentially `HashMap<VarId, VarInfo>`, but all [DefId]`s are consecutive.
    pub vars: Vec<VarInfo<'src>>,
    pub funcs: Vec<Func<'src>>,
    pub types: Vec<TypeDef<'src>>,
    pub stmts: Vec<Stmt<'src>>,
}

#[derive(Debug)]
pub struct VarInfo<'src> {
    pub span: Span<'src>,
    pub r#type: Type,
}

struct FuncSignature<'hir, 'src> {
    name: Name<'src>,
    generics: &'hir [Name<'src>],
    params: &'hir [Param<'src>],
    return_type: &'hir Spanned<'src, Type>,
}

const NOTHING_RETURN_TYPE: Spanned<Type> = (
    Type::Nothing,
    Span {
        start: 0,
        end: 0,
        filename: "<filename>",
        source: "<source>",
    },
);

// TODO: return (Option<TypedHir<'a>>, Vec<Error>) instead because the TypedHir may be recoverable on error
pub fn typed_hir_from_hir<'a>(mut hir: Hir<'a>) -> Result<TypedHir<'a>, Vec<Error<'a>>> {
    let vars = hir
        .vars
        .into_iter()
        .map(|var| {
            VarInfo {
                span: var.span,
                r#type: Type::Nothing, // Default value
            }
        })
        .collect();
    let (errors, vars) = {
        // Workaround to make the Rust compiler understand that no component of function
        // is actually borrowed mutably and immutably at the same time.
        let (func_signatures, func_bodies) = hir
            .funcs
            .iter_mut()
            .map(|func| {
                (
                    FuncSignature {
                        name: func.name,
                        generics: func.generics.as_slice(),
                        params: func.params.as_slice(),
                        return_type: func.return_type.as_ref().unwrap_or(&NOTHING_RETURN_TYPE),
                    },
                    &mut func.body,
                )
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();
        let mut checker = TypeChecker {
            errors: Errors::new(),
            vars,
            func_signatures: &func_signatures,
            current_func_signature: None,
            current_generics: &[],
            types: &hir.types,
        };
        for signature in &func_signatures {
            checker.check_func_signature(signature);
        }
        checker.check_stmts(&mut hir.stmts);
        checker.check_types();
        for (func_signature, func_body) in std::iter::zip(&func_signatures, func_bodies) {
            checker.current_func_signature = Some(func_signature);
            checker.with_generics(func_signature.generics, |checker| {
                checker.check_stmts(func_body)
            });
            checker.current_func_signature = None;
        }
        (checker.errors, checker.vars)
    };
    if !errors.is_empty() {
        return Err(errors.into_inner());
    }
    debug_assert!(vars.iter().all(|var| { var.r#type != Type::UNKNOWN }));
    Ok(TypedHir {
        vars,
        stmts: hir.stmts,
        funcs: hir.funcs,
        types: hir.types,
    })
}

struct TypeChecker<'src, 'hir> {
    errors: Errors<'src>,
    vars: Vec<VarInfo<'src>>,
    func_signatures: &'hir [FuncSignature<'hir, 'src>],
    current_func_signature: Option<&'hir FuncSignature<'hir, 'src>>,
    current_generics: &'hir [Name<'src>],
    types: &'hir [TypeDef<'src>],
}

impl<'src, 'hir> TypeChecker<'src, 'hir> {
    fn check_func_signature(&mut self, func: &'hir FuncSignature<'hir, 'src>) {
        self.current_func_signature = Some(func);
        self.with_generics(func.generics, |_self| {
            let FuncSignature {
                name,
                generics,
                params,
                return_type,
            } = *func;
            let mut ambiguous_generics = vec![true; generics.len()];
            for param in params {
                _self.check_type(&param.r#type);
                _self.vars[param.var_id.get()].r#type = param.r#type.0.clone();

                foreach_generic(&param.r#type.0, &mut |generic_id| {
                    // A generic that is used by a parameter is unambiguous because the passed argument determines its type.
                    ambiguous_generics[generic_id.get()] = false;
                });
            }
            foreach_generic(&return_type.0, &mut |generic_id| {
                if ambiguous_generics[generic_id.get()] {
                    _self.errors.add(
                        format!(
                            "Generic type '{}' is ambiguous. Generics in the return type of a function must also be used by a parameter",
                            generics[generic_id.get()].0,
                        ),
                        return_type.1,
                    )
                }
            });
            if name.0 == "main" {
                if !params.is_empty() {
                    _self.errors
                        .add("The 'main' function may not have any parameters", name.1);
                } else if not_equal(&return_type.0, &Type::Nothing) {
                    _self.errors
                        .add("The 'main' function may not return anything", name.1);
                }
            }
        });
        self.current_func_signature = None;
    }

    /// Checks the type definitions in the HIR.
    fn check_types(&mut self) {
        for type_def in self.types {
            match type_def {
                TypeDef::Struct(r#struct) => self.with_generics(&r#struct.generics, |_self| {
                    for field in &r#struct.fields {
                        _self.check_type(&field.r#type);
                    }
                }),
                TypeDef::Enum(r#enum) => self.with_generics(&r#enum.generics, |_self| {
                    for variant in &r#enum.variants {
                        if let Some(payload) = &variant.payload {
                            _self.check_type(payload);
                        }
                    }
                }),
            }
        }
    }

    fn check_stmt(&mut self, stmt: &mut Stmt<'src>) {
        match stmt {
            Stmt::VarDecl(VarDecl {
                var_id,
                name: _,
                value,
            }) => {
                let (r#type, _) = self.check_expr(value);
                self.vars[var_id.get()].r#type = r#type;
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr);
            }
            Stmt::Assign(Assign {
                op: (op, op_span),
                target,
                value,
            }) => {
                let (target_type, target_is_assignable) = self.check_expr(target);
                let (value_type, _) = self.check_expr(value);
                if !target_is_assignable {
                    self.errors.add(
                        "The left side of an assignment must be a variable, list or map element, or struct field",
                        target.1,
                    );
                }
                if not_equal(&target_type, &value_type) {
                    self.errors.add(
                        format!(
                            "Expected a value of type '{}', but found a value of type '{}'",
                            self.display(&target_type),
                            self.display(&value_type)
                        ),
                        value.1,
                    );
                }
                use AssignOp::*;
                match op {
                    Assign => {}
                    Add | Sub | Mul | Div | Mod => {
                        if not_equal(&target_type, &Type::Int) {
                            // TODO: simplify error message (also in Binary)
                            let message = format!(
                                "Cannot apply arithmetic operator to values of type '{}'",
                                self.display(&target_type)
                            );
                            self.errors.add(message, *op_span);
                        }
                    }
                }
            }
            Stmt::If(If { choices, r#else }) => {
                for Choice { cond, then } in choices {
                    let (cond_type, _) = self.check_expr(cond);
                    if not_equal(&cond_type, &Type::Bool) {
                        let message = format!(
                            "If-conditions must be booleans ('bool'). Found '{}'",
                            self.display(&cond_type)
                        );
                        self.errors.add(message, cond.1)
                    }
                    self.check_stmts(then);
                }
                if let Some(r#else) = r#else {
                    self.check_stmts(r#else);
                }
            }
            Stmt::Match(Match { expr, arms }) => {
                let (expr_type, _) = self.check_expr(expr);
                let Type::Named { id, generics } = &expr_type else {
                    self.errors.add(
                        format!(
                            "The value used for matching must be of an enum type. Found '{}'",
                            self.display(&expr_type)
                        ),
                        expr.1,
                    );
                    return;
                };
                let TypeDef::Enum(r#enum) = &self.types[id.get()] else {
                    self.errors.add(
                        format!(
                            "The value used for matching must be of an enum type, not a struct. Found '{}'",
                            self.display(&expr_type)
                        ),
                        expr.1,
                    );
                    return;
                };
                let mut used_variants = vec![None; r#enum.variants.len()];
                for arm in arms {
                    self.check_match_arm(arm, &mut used_variants, generics, r#enum);
                }
            }
            Stmt::For(For {
                var_id,
                var: _,
                iter,
                body,
            }) => {
                let (iter_type, _) = self.check_expr(iter);
                let var_type = match iter_type {
                    Type::Range => Type::Int,
                    Type::List(elem_type) => *elem_type,
                    Type::Map { key, value: _ } => *key,
                    _ => {
                        // TODO: add "Did you mean to use a range?" as context if iter_type==Type::Int
                        self.errors.add(
                            format!("Cannot iterate over type '{}'", self.display(&iter_type)),
                            iter.1,
                        );
                        Type::UNKNOWN
                    }
                };
                self.vars[var_id.get()].r#type = var_type;
                self.check_stmts(body);
            }
            Stmt::While(While { cond, body }) => {
                let (cond_type, _) = self.check_expr(cond);
                if not_equal(&cond_type, &Type::Bool) {
                    let message = format!(
                        "While-conditions must be booleans ('bool'), but found '{}'",
                        self.display(&cond_type)
                    );
                    self.errors.add(message, cond.1)
                }
                self.check_stmts(body);
            }
            Stmt::Loop(Loop { body }) => {
                self.check_stmts(body);
            }
            Stmt::Break(_) => {}
            Stmt::Continue(_) => {}
            Stmt::Return((value, span)) => {
                let value_type = value
                    .as_mut()
                    .map(|value| self.check_expr(value).0)
                    .unwrap_or(Type::Nothing);
                let expected_return_type = self
                    .current_func_signature
                    .map(|func| &func.return_type.0)
                    .unwrap_or(&Type::Nothing);
                if not_equal(&value_type, expected_return_type) {
                    // TODO: add function return type span as context
                    self.errors.add(
                        format!(
                            "Expected return type '{}', but found '{}'",
                            self.display(expected_return_type),
                            self.display(&value_type)
                        ),
                        value.as_ref().map(|(_, span)| *span).unwrap_or(*span),
                    );
                }
            }
        }
    }

    fn check_match_arm(
        &mut self,
        arm: &mut MatchArm<'src>,
        used_variants: &mut [Option<Span<'src>>],
        generics: &Vec<Type>,
        r#enum: &Enum<'src>,
    ) {
        let variants = &r#enum.variants;
        let MatchArm { pattern, body } = arm;
        match pattern {
            Pattern::Variant {
                variant_index,
                name,
                payload,
            } => {
                match variants.iter().position(|variant| variant.name.0 == name.0) {
                    Some(index) => {
                        *variant_index = index;
                        if let Some(prev_match_span) = used_variants[index] {
                            self.errors.add_with_context(
                                format!("The variant '{}' was already matched", name.0),
                                name.1,
                                "Previously matched here:",
                                prev_match_span,
                            );
                        }
                        used_variants[index] = Some(name.1);
                        match payload {
                            // TODO: error if def has payload and the match arm doesn't and the other way around
                            Some((var_id, payload_name)) => {
                                self.vars[var_id.get()].r#type = match variants[index]
                                    .payload
                                    .as_ref()
                                    .map(|(r#type, _)| r#type.with_generics(generics))
                                {
                                    Some(r#type) => r#type,
                                    None => {
                                        self.errors.add(
                                            format!(
                                                "The variant '{}' does not have a payload",
                                                name.0
                                            ),
                                            payload_name.1,
                                        );
                                        Type::UNKNOWN
                                    }
                                }
                            }
                            None => {
                                if variants[index].payload.is_some() {
                                    self.errors.add(
                                            format!(
                                                "The variant '{}' has a payload. Suggestion: use '{}(payload)'",
                                                name.0, name.0
                                            ),
                                            name.1,
                                        );
                                }
                            }
                        }
                    }
                    None => {
                        self.errors.add(
                            format!(
                                "The type '{}' does not have a variant named '{}'",
                                r#enum.name.0, name.0
                            ),
                            name.1,
                        );
                    }
                }
            }
        }
        self.check_stmts(body);
    }

    fn check_stmts(&mut self, stmts: &mut [Stmt<'src>]) {
        for stmt in stmts {
            self.check_stmt(stmt);
        }
    }

    /// Returns the expression's type and whether it can be assigned to.
    /// Expressions can only be assigned to if they are variables or non-empty lists/maps.
    fn check_expr(&mut self, (expr, span): &mut SpannedExpr<'src>) -> (Type, bool) {
        (
            match expr {
                Expr::Binary(binary) => self.check_binary((binary, *span)),
                Expr::Unary(unary) => self.check_unary((unary, *span)),
                Expr::Call(call) => self.check_call((call, *span)),
                Expr::EmptyList(EmptyList { element_type }) => {
                    self.check_type(element_type);
                    Type::List(Box::new(element_type.0.clone()))
                }
                Expr::List(list) => self.check_list((list, *span)),
                Expr::EmptyMap(EmptyMap {
                    key_type,
                    value_type,
                }) => {
                    self.check_type(key_type);
                    self.check_type(value_type);
                    Type::Map {
                        key: Box::new(key_type.0.clone()),
                        value: Box::new(value_type.0.clone()),
                    }
                }
                Expr::Map(map) => self.check_map((map, *span)),
                Expr::Index(index) => {
                    let is_assignable =
                        !matches!(index.expr.0, Expr::EmptyList(_) | Expr::EmptyMap(_));
                    return (self.check_index((index, *span)), is_assignable);
                }
                Expr::Member(member) => {
                    return (self.check_member((member, *span)), true);
                }
                Expr::Var(var_id) => return (self.vars[var_id.get()].r#type.clone(), true),
                Expr::Int(_) => Type::Int,
                Expr::Bool(_) => Type::Bool,
                Expr::String(_) => Type::string(),
                Expr::Char(_) => Type::Char,
                Expr::Range(Range { start, end }) => {
                    self.check_expr(start);
                    self.check_expr(end);
                    Type::Range
                }
                Expr::StructLiteral(StructLiteral {
                    type_id,
                    name: _,
                    fields,
                }) => {
                    let TypeDef::Struct(r#struct) = &self.types[type_id.get()] else {
                        unreachable!();
                    };
                    let generics = &r#struct.generics;
                    let mut generic_constraints = HashMap::new();
                    // NOTE: this assumes that the field and struct field have been matched in HIR
                    //       (which may not be true if error recovery is used)
                    for (field, struct_field) in std::iter::zip(fields, &r#struct.fields) {
                        let (field_type, _) = self.check_expr(&mut field.value);
                        if does_not_conform_to(&field_type, &struct_field.r#type.0) {
                            self.errors.add(
                                format!(
                                    "The field '{}' has type '{}', but the provided value has type '{}'",
                                    struct_field.name.0,
                                    self.display_with_generics(&struct_field.r#type.0, &r#struct.generics),
                                    self.display(&field_type),
                                ),
                                field.value.1,
                            );
                        } else if !generics.is_empty() {
                            extract_generic_constraints(
                                &field_type,
                                field.value.1,
                                &struct_field.r#type.0,
                                struct_field.name.1,
                                &mut generic_constraints,
                            );
                        }
                    }
                    self.verify_generic_constraints(
                        &generic_constraints,
                        generics,
                        r#struct.name,
                        "field",
                        "struct",
                    );
                    Type::Named {
                        id: *type_id,
                        generics: generic_values_from_constraints(
                            generics.len(),
                            &generic_constraints,
                        ),
                    }
                }
                Expr::VariantLiteral(VariantLiteral {
                    type_id,
                    variant_index,
                    enum_name: _,
                    name: _,
                    payload,
                }) => {
                    let TypeDef::Enum(r#enum) = &self.types[type_id.get()] else {
                        unreachable!();
                    };
                    let expected_payload_type = r#enum.variants[*variant_index].payload.as_ref();
                    let payload_type = payload
                        .as_mut()
                        .map(|payload| (self.check_expr(payload).0, payload.1));
                    let generics = &r#enum.generics;
                    let mut generic_constraints = HashMap::new();
                    match (expected_payload_type, payload_type) {
                        (Some((expected, expected_span)), Some((found, found_span))) => {
                            if does_not_conform_to(&found, expected) {
                                self.errors.add(
                                    format!(
                                        "Expected '{}' as payload type, but found '{}'",
                                        self.display_with_generics(&expected, &r#enum.generics),
                                        self.display(&found)
                                    ),
                                    found_span,
                                );
                            } else if !generics.is_empty() {
                                extract_generic_constraints(
                                    &found,
                                    found_span,
                                    expected,
                                    *expected_span,
                                    &mut generic_constraints,
                                );
                            }
                        }
                        (Some((expected, _)), None) => {
                            self.errors.add(
                                format!(
                                    "This enum variant requires a payload of type '{}'",
                                    self.display_with_generics(&expected, &r#enum.generics)
                                ),
                                *span,
                            );
                        }
                        (None, Some((_, payload_span))) => {
                            self.errors
                                .add("This enum variant does not expect a payload", payload_span);
                        }
                        (None, None) => {} // OK
                    }
                    self.verify_generic_constraints(
                        &generic_constraints,
                        generics,
                        r#enum.name,
                        "value",
                        "enum",
                    );
                    Type::Named {
                        id: *type_id,
                        generics: generic_values_from_constraints(
                            generics.len(),
                            &generic_constraints,
                        ),
                    }
                }
                Expr::FormatString(FormatString {
                    segments,
                    end_text: _,
                }) => {
                    for (_, expr) in segments {
                        let _ = self.check_expr(expr);
                    }
                    Type::string()
                }
            },
            false,
        )
    }

    fn check_binary(&mut self, (binary, span): Spanned<'src, &mut Binary<'src>>) -> Type {
        let (left_type, _) = self.check_expr(&mut binary.left);
        let (right_type, _) = self.check_expr(&mut binary.right);
        let or_type = left_type.or(&right_type); // Known if either one is known

        if not_equal(&left_type, &right_type) {
            self.errors.add(
                format!(
                    "Both sides must be of the same type. Left is '{}' and right is '{}'",
                    self.display(&left_type),
                    self.display(&right_type)
                ),
                span,
            );
        }
        use BinaryOp::*;
        match binary.op.0 {
            Add | Sub | Div | Mul | Mod => {
                if not_equal(or_type, &Type::Int) {
                    self.errors.add(
                        format!(
                            "Cannot apply arithmetic operator to values of type '{}'",
                            self.display(&or_type)
                        ),
                        binary.op.1,
                    );
                }
                or_type.clone()
            }
            Lt | Leq | Gt | Geq => {
                if not_equal(or_type, &Type::Int) {
                    self.errors.add(
                        format!(
                            "Cannot apply comparison operator to values of type '{}'",
                            self.display(&or_type)
                        ),
                        binary.op.1,
                    );
                }
                Type::Bool
            }
            And | Or => {
                if not_equal(or_type, &Type::Bool) {
                    self.errors.add(
                        format!(
                            "Cannot apply boolean operator to values of type '{}'",
                            self.display(&or_type)
                        ),
                        binary.op.1,
                    );
                }
                Type::Bool
            }
            Eq | NotEq => Type::Bool,
        }
    }

    fn check_unary(&mut self, (unary, span): Spanned<'src, &mut Unary<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&mut unary.expr);
        match unary.op.0 {
            UnaryOp::Neg => {
                if not_equal(&expr_type, &Type::Int) {
                    self.errors.add(
                        format!(
                            "Only integers can be negated. Cannot negate '{}'",
                            self.display(&expr_type)
                        ),
                        span,
                    );
                }
            }
            UnaryOp::Not => {
                if not_equal(&expr_type, &Type::Bool) {
                    self.errors.add(
                        format!(
                            "Only booleans can be inverted. Cannot invert '{}'",
                            self.display(&expr_type)
                        ),
                        span,
                    );
                }
            }
        }
        expr_type
    }

    fn check_call(&mut self, (call, span): Spanned<'src, &mut Call<'src>>) -> Type {
        if call.func_id == FuncId::PRINT {
            for arg in &mut call.args {
                self.check_expr(arg);
            }
            // print(...) accepts any number of arguments of any type.
            return Type::Nothing;
        }
        let &FuncSignature {
            name: func_name,
            generics,
            params,
            return_type,
        } = &self.func_signatures[call.func_id.get()];
        let args = &mut call.args;
        if params.len() > args.len() {
            let closing_paren_span = Span {
                start: span.end - 1,
                ..span
            };
            let extra_needed = params.len() - args.len();
            let message = format!(
                "This function needs {} more argument{}",
                extra_needed,
                s_if_not_1(extra_needed)
            );
            self.errors.add(message, closing_paren_span);
        }
        if params.len() < args.len() {
            let (_, first_arg_after_params_span) = args[params.len()];
            let (_, last_arg_span) = args.last().unwrap();
            let after_params_span = Span {
                start: first_arg_after_params_span.start,
                ..*last_arg_span
            };
            let message = format!(
                "This function only takes {} argument{}. {} argument{} passed",
                params.len(),
                s_if_not_1(params.len()),
                args.len(),
                s_if_not_1(params.len()),
            );
            self.errors.add(message, after_params_span);
        }
        let mut generic_constraints = HashMap::new();
        for (arg, param) in std::iter::zip(args, params) {
            let (arg_type, _) = self.check_expr(arg);
            let (param_type, _) = &param.r#type;
            if not_equal(&arg_type, param_type) {
                let message = format!(
                    "Expected a value of type '{}' but found a value of type '{}'",
                    self.display(&arg_type),
                    self.display(&param_type)
                );
                self.errors.add(message, arg.1);
            } else if !generics.is_empty() {
                extract_generic_constraints(
                    &arg_type,
                    arg.1,
                    param_type,
                    param.name.1,
                    &mut generic_constraints,
                );
            }
        }
        self.verify_generic_constraints(
            &generic_constraints,
            generics,
            func_name,
            "argument",
            "function",
        );
        return_type.0.clone()
    }

    fn check_list(&mut self, (list, span): Spanned<'src, &mut List<'src>>) -> Type {
        if list.elements.is_empty() {
            self.errors.add(
                "An empty list must have an element type specified. Example: [<int>]",
                span,
            );
            return Type::UNKNOWN;
        }
        let (first_element_type, _) = self.check_expr(&mut list.elements[0]);
        let first_element_span = list.elements[0].1;
        for element in &mut list.elements[1..] {
            let (element_type, _) = self.check_expr(element);
            if not_equal(&first_element_type, &element_type) {
                self.errors.add_with_context(
                    format!(
                        "All elements of a list should have the same type, but this one has type '{}'",
                        self.display(&    element_type)
                    ),
                    element.1,
                    format!("While the first element has type '{}'", self.display(&first_element_type)),
                    first_element_span,
                );
            }
        }
        Type::List(Box::new(first_element_type))
    }

    fn check_map(&mut self, (map, span): Spanned<'src, &mut Map<'src>>) -> Type {
        if map.pairs.is_empty() {
            self.errors.add(
                "An empty map must have a key and value type specified. Example: {<string, int>}",
                span,
            );
            return Type::Map {
                key: Box::new(Type::UNKNOWN),
                value: Box::new(Type::UNKNOWN),
            };
        }
        let (first_key_type, _) = self.check_expr(&mut map.pairs[0].key);
        let (first_value_type, _) = self.check_expr(&mut map.pairs[0].value);
        let first_key_span = map.pairs[0].key.1;
        let first_value_span = map.pairs[0].value.1;
        for Pair { key, value } in &mut map.pairs[1..] {
            macro_rules! check {
                ($first_type:expr, $first_span:expr, $var:ident, $str:literal) => {{
                    let (r#type, _) = self.check_expr($var);
                    if not_equal($first_type, &r#type) {
                        self.errors.add_with_context(
                            format!(
                                "All map {}s should have the same type, but this {} has type '{}'",
                                $str,
                                $str,
                                self.display(&r#type)
                            ),
                            $var.1,
                            format!(
                                "While the first {} has type '{}'",
                                $str,
                                self.display(&$first_type)
                            ),
                            $first_span,
                        );
                    }
                }};
            }
            check!(&first_key_type, first_key_span, key, "key");
            check!(&first_value_type, first_value_span, value, "value");
        }
        Type::Map {
            key: Box::new(first_key_type),
            value: Box::new(first_value_type),
        }
    }

    fn check_index(&mut self, (index, span): Spanned<'src, &mut Index<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&mut index.expr);
        let (index_type, _) = self.check_expr(&mut index.index);
        match &index.expr.0 {
            // TODO: also detect this when index.expr is a variable instead
            Expr::EmptyList(_) => self.errors.add(
                format!("Getting an element from an empty list always fails"),
                span,
            ),
            Expr::EmptyMap(_) => self.errors.add(
                format!("Getting a value from an empty map always fails"),
                span,
            ),
            _ => {}
        }
        match expr_type {
            Type::List(ref element_type) => match index_type {
                Type::Int => element_type.deref().clone(),
                Type::Range => expr_type,
                index_type => {
                    if index_type.is_known() {
                        self.errors.add(
                            format!(
                                "Only int and range can be used to index into lists and strings, not '{}'",
                                self.display(&index_type)
                            ),
                            index.index.1,
                        );
                    }
                    Type::UNKNOWN
                }
            },
            Type::Map {
                key: key_type,
                value: value_type,
            } => {
                if not_equal(&index_type, &key_type) {
                    self.errors.add(
                        format!(
                            "The map's key type is '{}', but '{}' was used to access it",
                            self.display(&key_type),
                            self.display(&index_type)
                        ),
                        index.index.1,
                    );
                }
                *value_type
            }
            expr_type => {
                self.errors.add(
                    format!(
                        "Only 'string', 'list', and 'map' can be indexed, not '{}'",
                        self.display(&expr_type)
                    ),
                    index.expr.1,
                );
                Type::UNKNOWN
            }
        }
    }

    fn check_member(&mut self, (member, _): Spanned<&mut Member<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&mut member.expr);
        if !expr_type.is_known() {
            return Type::UNKNOWN;
        }
        let Type::Named { id, generics: _ } = expr_type else {
            self.errors.add(
                format!(
                    "Cannot access field of non-struct type '{}'",
                    self.display(&expr_type)
                ),
                member.expr.1,
            );
            return Type::UNKNOWN;
        };
        member.type_id = id;
        let TypeDef::Struct(r#struct) = &self.types[id.get()] else {
            self.errors.add(
                format!(
                    "Cannot access field of enum type '{}'",
                    self.display(&expr_type)
                ),
                member.expr.1,
            );
            return Type::UNKNOWN;
        };
        match r#struct
            .fields
            .iter()
            .position(|field| field.name.0 == member.member.0)
        {
            Some(field_index) => {
                member.field_index = field_index;
                r#struct.fields[field_index].r#type.0.clone()
            }
            None => {
                self.errors.add(
                    format!(
                        "Struct '{}' does not have a field named '{}'",
                        r#struct.name.0, member.member.0
                    ),
                    member.member.1,
                );
                Type::UNKNOWN
            }
        }
    }

    fn check_type(&mut self, (r#type, span): &Spanned<'src, Type>) {
        let Type::Named { id, generics } = r#type else {
            return;
        };
        let def = &self.types[id.get()];
        let def_generics_len = 0;
        // TODO: add generics to structs/enums

        if generics.len() != def_generics_len {
            self.errors.add(
                format!(
                    "The type '{}' has {} generics, but {} were provided",
                    def.name().0,
                    def_generics_len,
                    generics.len()
                ),
                *span,
            );
        }
    }

    fn with_generics<T>(
        &mut self,
        generics: &'hir [Name<'src>],
        func: impl FnOnce(&mut Self) -> T,
    ) -> T {
        debug_assert!(self.current_generics.is_empty());
        self.current_generics = generics;
        let value = func(self);
        self.current_generics = &[];
        value
    }

    fn verify_generic_constraints(
        &mut self,
        constraints: &HashMap<GenericId, Vec<(Type, Span<'src>)>>,
        def_generics: &[Name<'src>],
        def_name: Name<'src>,
        item_text: &'static str,
        def_text: &'static str,
    ) {
        for (generic_id, pairs) in constraints {
            if pairs.len() <= 1 {
                continue;
            }
            let (first_type, first_span) = &pairs[0];
            for (r#type, span) in &pairs[1..] {
                if r#type == first_type {
                    continue;
                }
                let generic = def_generics[generic_id.get()];
                if span == first_span {
                    self.errors.add_with_context(
                        // TODO: change text 'argument'/'function' when enum/struct is used
                        format!(
                            "This {item_text} requires that generic '{}' of {def_text} '{}' is both '{}' and '{}'",
                            generic.0,
                            def_name.0,
                            self.display(&first_type),
                            self.display(&r#type),
                        ),
                        *span,
                        "Generic defined here:",
                        generic.1,
                    );
                } else {
                    self.errors.add_with_contexts(
                        format!(
                            "This {item_text} requires that generic '{}' of {def_text} '{}' is '{}', but another requires that it is '{}'",
                            generic.0,
                            def_name.0,
                            self.display(&first_type),
                            self.display(&r#type),
                        ),
                        *first_span,
                        vec![
                            ("The other argument:".to_string(), *span),
                            ("Generic defined here:".to_string(), generic.1)
                        ],
                    );
                }
                break;
            }
        }
    }

    fn display_with_generics<'a>(
        &'a self,
        r#type: &'a Type,
        generics: &'a [Name<'a>],
    ) -> Display<'a, 'a, &'a Type> {
        Display {
            types: &self.types,
            generic_names: generics,
            value: r#type,
        }
    }

    fn display<'a>(&'a self, r#type: &'a Type) -> Display<'a, 'a, &'a Type> {
        self.display_with_generics(r#type, self.current_generics)
    }
}

fn generic_values_from_constraints(
    num_generics: usize,
    generic_constraints: &HashMap<GenericId, Vec<(Type, Span<'_>)>>,
) -> Vec<Type> {
    (0..num_generics)
        .map(|i| match generic_constraints.get(&GenericId(i)) {
            Some(r#types) => r#types[0].0.clone(),
            None => Type::UnknownGeneric,
        })
        .collect()
}

/// Returns a literal string of "s" if the given value is exactly 1.
/// Useful for conditionally pluralizing words.
fn s_if_not_1(n: usize) -> &'static str {
    if n != 1 { "s" } else { "" }
}

/// Returns `true` if the types are definitively not equal.
///
/// This function also returns `false` if either type is unknown.
fn not_equal(left: &Type, right: &Type) -> bool {
    left.is_known() && right.is_known() && left != right
}

/// Returns `true` if `r#type` does not conform to `other`.
/// This means that the types are not equal *and* `other` is not a generic type.
///
/// This function also returns `false` if either type is unknown.
fn does_not_conform_to(r#type: &Type, other: &Type) -> bool {
    r#type.is_known() && other.is_known() && !matches!(other, Type::Generic(_)) && r#type != other
}

fn extract_generic_constraints<'src>(
    value_type: &Type,
    value_span: Span<'src>,
    def_type: &Type,
    def_span: Span<'src>,
    constraints: &mut HashMap<GenericId, Vec<(Type, Span<'src>)>>,
) {
    if !value_type.is_known() || !def_type.is_known() {
        return;
    }
    match (value_type, def_type) {
        (Type::Generic(generic_id), _) => {
            constraints
                .entry(*generic_id)
                .or_default()
                .push((def_type.clone(), def_span));
        }
        (Type::List(value_element), Type::List(def_element)) => {
            extract_generic_constraints(
                value_element,
                value_span,
                def_element,
                def_span,
                constraints,
            );
        }
        (
            Type::Map {
                key: value_key,
                value: value_value,
            },
            Type::Map {
                key: def_key,
                value: def_value,
            },
        ) => {
            extract_generic_constraints(value_key, value_span, def_key, def_span, constraints);
            extract_generic_constraints(value_value, value_span, def_value, def_span, constraints);
        }
        (
            Type::Named {
                id: value_id,
                generics: value_generics,
            },
            Type::Named {
                id: def_id,
                generics: def_generics,
            },
        ) if *value_id != TypeId::UNKNOWN && value_id == def_id => {
            for (value_generic, def_generic) in std::iter::zip(value_generics, def_generics) {
                extract_generic_constraints(
                    value_generic,
                    value_span,
                    def_generic,
                    def_span,
                    constraints,
                );
            }
        }
        _ => {}
    }
}

/// Calls `func` for every generic in `r#type`.
fn foreach_generic(r#type: &Type, func: &mut impl FnMut(GenericId)) {
    match r#type {
        Type::List(element) => foreach_generic(element, func),
        Type::Map { key, value } => {
            foreach_generic(key, func);
            foreach_generic(value, func);
        }
        Type::Named { id: _, generics } => generics
            .iter()
            .for_each(|generic| foreach_generic(generic, func)),
        Type::Generic(generic_id) => func(*generic_id),
        _ => {}
    }
}

struct Display<'hir, 'src, T> {
    types: &'hir [TypeDef<'src>],
    generic_names: &'hir [(&'src str, Span<'src>)],
    value: T,
}

impl<T> Display<'_, '_, T> {
    fn with(&self, value: T) -> Self {
        Self { value, ..*self }
    }
}

impl std::fmt::Display for Display<'_, '_, &'_ Type> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            types,
            generic_names,
            value: r#type,
        } = self;
        match r#type {
            Type::Nothing | Type::Int | Type::Bool | Type::Char | Type::Range => {
                hir::Display(*r#type).fmt(f)
            }
            Type::List(element) => {
                if **element == Type::Char {
                    f.write_str("string")
                } else {
                    write!(f, "list<{}>", self.with(element))
                }
            }
            Type::Map { key, value } => {
                write!(f, "map<{}, {}>", self.with(key), self.with(value))
            }
            Type::Named { id, generics } => {
                let name = types[id.get()].name();
                f.write_str(name.0)?;
                if !generics.is_empty() {
                    f.write_str("<")?;
                    write_comma_separated(f, generics.iter().map(|generic| self.with(generic)))?;
                    f.write_str(">")?;
                }
                Ok(())
            }
            Type::Generic(generic_id) => {
                let name = generic_names[generic_id.get()];
                f.write_str(name.0)
            }
            Type::UnknownGeneric => f.write_str("?"),
        }
    }
}
