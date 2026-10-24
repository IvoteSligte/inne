use std::ops::Deref;

pub use crate::hir::{
    Assign, AssignOp, Binary, BinaryOp, Block, Call, Choice, EmptyList, EmptyMap, Expr, For, Func,
    FuncId, Hir, If, Index, List, Loop, Map, Member, Name, Pair, Param, Range, Stmt, Struct,
    StructLiteral, Tuple, Type, TypeId, Unary, UnaryOp, VarDecl, VarId, While,
};

use crate::{Error, Span, Spanned, hir::Errors};

// NOTE: The HIR only needs to be edited to set Member.member_index,
//       but maybe in the future a whole transformation is better?

/// Type-checked high-level intermediate representation.
/// Contains type annotations for all function and variable definitions.
pub struct TypedHir<'a> {
    pub filename: &'a str,
    /// Essentially `HashMap<VarId, VarInfo>`, but all [DefId]`s are consecutive.
    pub vars: Vec<VarInfo>,
    pub funcs: Vec<Func<'a>>,
    pub types: Vec<Struct<'a>>,
    pub stmts: Vec<Stmt<'a>>,
}

pub struct VarInfo {
    pub span: Span,
    pub r#type: Type,
}

// TODO: return (Option<TypedHir<'a>>, Vec<Error>) instead because the TypedHir may be recoverable on error
pub fn typed_hir_from_hir<'a>(mut hir: Hir<'a>) -> Result<TypedHir<'a>, Vec<Error>> {
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
                    (
                        func.name,
                        func.params.as_slice(),
                        func.return_type
                            .as_ref()
                            .map(|(r#type, _)| r#type)
                            .unwrap_or(&Type::Nothing),
                    ),
                    &mut func.body,
                )
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();
        let mut type_checker = TypeChecker {
            errors: Errors::new(),
            vars,
            func_signatures: &func_signatures,
            types: &hir.types,
            expected_return_type: &Type::Nothing,
        };
        for (name, params, return_type) in &func_signatures {
            type_checker.check_func_signature(*name, params, return_type);
        }
        type_checker.check_stmts(&mut hir.stmts);
        type_checker.check_types();
        for ((_, _, func_return_type), func_body) in std::iter::zip(&func_signatures, func_bodies) {
            type_checker.expected_return_type = func_return_type;
            type_checker.check_stmts(func_body);
        }
        (type_checker.errors, type_checker.vars)
    };
    if !errors.is_empty() {
        return Err(errors.into_inner());
    }
    debug_assert!(vars.iter().all(|var| var.r#type.is_known()));
    Ok(TypedHir {
        filename: hir.filename,
        vars,
        stmts: hir.stmts,
        funcs: hir.funcs,
        types: hir.types,
    })
}

struct TypeChecker<'src, 'hir> {
    errors: Errors,
    vars: Vec<VarInfo>,
    func_signatures: &'hir [(Name<'src>, &'hir [Param<'src>], &'hir Type)],
    types: &'hir [Struct<'src>],
    expected_return_type: &'hir Type,
}

impl<'src, 'hir> TypeChecker<'src, 'hir> {
    fn check_func_signature(
        &mut self,
        name: Name<'src>,
        params: &'hir [Param<'src>],
        return_type: &'hir Type,
    ) {
        for param in params {
            self.check_type(&param.r#type);
            self.vars[param.var_id.get()].r#type = param.r#type.0.clone();
        }
        if name.0 == "main" {
            if !params.is_empty() {
                self.errors
                    .add("The 'main' function may not have any parameters.", name.1);
            } else if *return_type != Type::Nothing {
                self.errors
                    .add("The 'main' function may not return anything.", name.1);
            }
        }
    }

    /// Checks the type definitions in the HIR.
    fn check_types(&mut self) {
        for r#struct in self.types {
            for field in &r#struct.fields {
                self.check_type(&field.r#type);
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
                        "The left side of an assignment must be a variable, list or map element, or struct field.",
                        // FIXME: this span does not include the .expr part of an index?
                        target.1,
                    );
                }
                if target_type.is_known() && value_type.is_known() && target_type != value_type {
                    self.errors.add(
                        format!(
                            "Expected a value of type '{}', but found a value of type '{}'",
                            target_type, value_type
                        ),
                        value.1,
                    );
                }
                if target_type.is_known() {
                    use AssignOp::*;
                    match op {
                        Assign => {}
                        Add | Sub | Mul | Div | Mod => {
                            if target_type != Type::Int {
                                // TODO: simplify error message (also in Binary)
                                let message = format!(
                                    "Cannot apply arithmetic operator to values of type '{}'",
                                    target_type
                                );
                                self.errors.add(message, *op_span);
                            }
                        }
                    }
                }
            }
            Stmt::If(If { choices, r#else }) => {
                for Choice { cond, then } in choices {
                    let (cond_type, _) = self.check_expr(cond);
                    if cond_type != Type::Bool {
                        let message = format!(
                            "If-conditions must be booleans ('bool'). Found '{}'.",
                            cond_type
                        );
                        self.errors.add(message, cond.1)
                    }
                    self.check_stmts(then);
                }
                if let Some(r#else) = r#else {
                    self.check_stmts(r#else);
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
                    Type::String => Type::Char,
                    Type::List(elem_type) => *elem_type,
                    Type::Map { key, value } => Type::Tuple(vec![*key, *value]),
                    _ => {
                        // TODO: add "Did you mean to use a range?" as context if iter_type==Type::Int
                        self.errors
                            .add(format!("Cannot iterate over type '{}'", iter_type), iter.1);
                        Type::UNKNOWN
                    }
                };
                self.vars[var_id.get()].r#type = var_type;
                self.check_stmts(body);
            }
            Stmt::While(While { cond, body }) => {
                let (cond_type, _) = self.check_expr(cond);
                if cond_type != Type::Bool {
                    let message = format!(
                        "While-conditions must be booleans ('bool'). Found '{}'.",
                        cond_type
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
                if value_type != *self.expected_return_type {
                    // TODO: add function return type span as context
                    self.errors.add(
                        format!(
                            "Expected return type '{}', but found '{}'",
                            self.expected_return_type, value_type
                        ),
                        value.as_ref().map(|(_, span)| *span).unwrap_or(*span),
                    );
                }
            }
        }
    }

    fn check_stmts(&mut self, stmts: &mut [Stmt<'src>]) {
        for stmt in stmts {
            self.check_stmt(stmt);
        }
    }

    /// Returns the expression's type and whether it can be assigned to.
    /// Expressions can only be assigned to if they are variables or non-empty lists/maps.
    fn check_expr(&mut self, (expr, span): &mut Spanned<Expr<'src>>) -> (Type, bool) {
        (
            match expr {
                Expr::Binary(binary) => self.check_binary((binary, *span)),
                Expr::Unary(unary) => self.check_unary((unary, *span)),
                Expr::Call(call) => self.check_call((call, *span)),
                Expr::EmptyList(EmptyList { element_type }) => {
                    self.check_type(element_type);
                    element_type.0.clone()
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
                Expr::Tuple(tuple) => self.check_tuple((tuple, *span)),
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
                Expr::String(_) => Type::String,
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
                    let r#struct = &self.types[type_id.get()];

                    // NOTE: this assumes that the field and struct field have been matched in HIR
                    //       (which may not be true if error recovery is used)
                    for (field, struct_field) in std::iter::zip(fields, &r#struct.fields) {
                        let (value_type, _) = self.check_expr(&mut field.value);
                        if types_not_equal(&value_type, &struct_field.r#type.0) {
                            self.errors.add(
                                format!(
                                    "The field '{}' has type '{}', but the provided value has type '{}'",
                                    struct_field.name.0,
                                    struct_field.r#type.0,
                                    value_type,
                                ),
                                field.value.1,
                            );
                        }
                    }
                    Type::Named {
                        id: *type_id,
                        generics: Vec::new(), // NOTE: currently, structs do not have generics
                    }
                }
            },
            false,
        )
    }

    fn check_binary(&mut self, (binary, span): Spanned<&mut Binary<'src>>) -> Type {
        let (left_type, _) = self.check_expr(&mut binary.left);
        let (right_type, _) = self.check_expr(&mut binary.right);
        let or_type = left_type.or(&right_type); // Known if either one is known

        if or_type.is_known() && left_type != right_type {
            self.errors.add(
                format!(
                    "Both sides must be of the same type. Left is '{}' and right is '{}'.",
                    left_type, right_type,
                ),
                span,
            );
        }
        use BinaryOp::*;
        match binary.op.0 {
            Add | Sub | Div | Mul | Mod => {
                if or_type.is_known() && *or_type != Type::Int {
                    self.errors.add(
                        format!(
                            "Cannot apply arithmetic operator to values of type '{}'",
                            or_type
                        ),
                        binary.op.1,
                    );
                }
                or_type.clone()
            }
            Lt | Leq | Gt | Geq => {
                if or_type.is_known() && *or_type != Type::Int {
                    self.errors.add(
                        format!(
                            "Cannot apply comparison operator to values of type '{}'",
                            or_type
                        ),
                        binary.op.1,
                    );
                }
                Type::Bool
            }
            And | Or => {
                if or_type.is_known() && *or_type != Type::Bool {
                    self.errors.add(
                        format!(
                            "Cannot apply boolean operator to values of type '{}'",
                            or_type
                        ),
                        binary.op.1,
                    );
                }
                Type::Bool
            }
            Eq | NotEq => Type::Bool,
        }
    }

    fn check_unary(&mut self, (unary, span): Spanned<&mut Unary<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&mut unary.expr);
        if expr_type.is_unknown() {
            return expr_type;
        }
        match unary.op.0 {
            UnaryOp::Neg => {
                if expr_type != Type::Int {
                    self.errors.add(
                        format!(
                            "Only integers can be negated. Cannot negate '{}'.",
                            expr_type
                        ),
                        span,
                    );
                }
            }
            UnaryOp::Not => {
                if expr_type != Type::Bool {
                    self.errors.add(
                        format!(
                            "Only booleans can be inverted. Cannot invert '{}'.",
                            expr_type
                        ),
                        span,
                    );
                }
            }
        }
        expr_type
    }

    fn check_call(&mut self, (call, span): Spanned<&mut Call<'src>>) -> Type {
        if call.func_id == FuncId::PRINT {
            // print(...) accepts any number of arguments of any type.
            return Type::Nothing;
        }
        let (_, params, return_type) = self.func_signatures[call.func_id.get()];
        let args = &mut call.args;
        if params.len() > args.len() {
            let closing_paren_span = Span {
                start: span.end - 1,
                end: span.end,
                context: span.context,
            };
            let message = format!(
                "This function needs {} more arguments.",
                params.len() - args.len()
            );
            self.errors.add(message, closing_paren_span);
        }
        if params.len() < args.len() {
            let (_, first_arg_after_params_span) = args[params.len()];
            let (_, last_arg_span) = args.last().unwrap();
            let after_params_span = Span {
                start: first_arg_after_params_span.start,
                end: last_arg_span.end,
                context: last_arg_span.context,
            };
            let message = format!(
                "This function only takes {} arguments. {} arguments passed.",
                params.len(),
                args.len()
            );
            self.errors.add(message, after_params_span);
        }
        for (arg, param) in std::iter::zip(args, params) {
            let (arg_type, _) = self.check_expr(arg);
            let (param_type, _) = &param.r#type;
            if arg_type.is_known() && arg_type != *param_type {
                let message = format!(
                    "Expected a value of type '{}' but found a value of type '{}'",
                    arg_type, param_type
                );
                self.errors.add(message, arg.1);
            }
        }
        return_type.clone()
    }

    fn check_list(&mut self, (list, span): Spanned<&mut List<'src>>) -> Type {
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
            if types_not_equal(&first_element_type, &element_type) {
                self.errors.add_with_context(
                    format!(
                        "All list elements should have the same type, but this one has type '{}'",
                        element_type
                    ),
                    element.1,
                    format!("The first element has type '{}'", first_element_type),
                    first_element_span,
                );
            }
        }
        Type::List(Box::new(first_element_type))
    }

    fn check_map(&mut self, (map, span): Spanned<&mut Map<'src>>) -> Type {
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
                    if types_not_equal($first_type, &r#type) {
                        self.errors.add_with_context(
                            format!(
                                "All map {}s should have the same type, but this {} has type '{}'",
                                $str, $str, r#type
                            ),
                            $var.1,
                            format!("The first {} has type '{}'", $str, $first_type),
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

    fn check_tuple(&mut self, (tuple, _): Spanned<&mut Tuple<'src>>) -> Type {
        let elements = tuple
            .elements
            .iter_mut()
            .map(|elem| self.check_expr(elem).0)
            .collect();
        Type::Tuple(elements)
    }

    fn check_index(&mut self, (index, span): Spanned<&mut Index<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&mut index.expr);
        let (index_type, _) = self.check_expr(&mut index.index);
        match &index.expr.0 {
            // TODO: also detect this when index.expr is a variable instead
            Expr::EmptyList(_) => self.errors.add(
                format!("Getting an element from an empty list always fails."),
                span,
            ),
            Expr::EmptyMap(_) => self.errors.add(
                format!("Getting a value from an empty map always fails."),
                span,
            ),
            _ => {}
        }
        match expr_type {
            Type::String => match index_type {
                Type::Int => Type::Char,
                Type::Range => Type::String,
                index_type => {
                    if index_type.is_known() {
                        self.errors.add(format!("Only int and range can be used to index into strings, not '{index_type}'."), index.index.1);
                    }
                    Type::UNKNOWN
                }
            },
            Type::List(ref element_type) => match index_type {
                Type::Int => element_type.deref().clone(),
                Type::Range => expr_type,
                index_type => {
                    if index_type.is_known() {
                        self.errors.add(format!("Only int and range can be used to index into lists, not '{index_type}'."), index.index.1);
                    }
                    Type::UNKNOWN
                }
            },
            Type::Map {
                key: key_type,
                value: value_type,
            } => {
                if index_type.is_known() && key_type.is_known() && index_type != *key_type {
                    self.errors.add(format!("The map's key type is '{key_type}', but '{index_type}' was used to access it."), index.index.1);
                }
                *value_type
            }
            expr_type => {
                self.errors.add(
                    format!("Only 'string', 'list', and 'map' can be indexed, not '{expr_type}'."),
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
                format!("Cannot access field of non-struct type '{}'", expr_type),
                member.expr.1,
            );
            return Type::UNKNOWN;
        };
        member.type_id = id;
        let r#struct = &self.types[id.get()];
        match r#struct
            .fields
            .iter()
            .position(|field| field.name.0 == member.member.0)
        {
            Some(member_index) => {
                member.field_index = member_index;
                r#struct.fields[member_index].r#type.0.clone()
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

    fn check_type(&mut self, (r#type, span): &Spanned<Type>) {
        let Type::Named { id, generics } = r#type else {
            return;
        };
        let def = &self.types[id.get()];
        let def_generics_len = 0;
        // TODO: add generics to structs

        if generics.len() != def_generics_len {
            self.errors.add(
                format!(
                    "The type '{}' has {} generics, but {} were provided.",
                    def.name.0,
                    def_generics_len,
                    generics.len()
                ),
                *span,
            );
        }
    }
}

/// Returns `true` if the types are definitively not equal.
///
/// This function also returns `false` if either type is unknown.
fn types_not_equal(left: &Type, right: &Type) -> bool {
    left.is_known() && right.is_known() && left != right
}
