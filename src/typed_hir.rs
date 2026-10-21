use std::{num::IntErrorKind, ops::Deref};

pub use crate::hir::{
    Assign, AssignOp, Binary, BinaryOp, Block, Call, Choice, DefId, EmptyList, EmptyMap, Expr, For,
    Func, Hir, If, Index, List, Loop, Map, Pair, Param, Range, Stmt, Type, Unary, UnaryOp, VarDecl,
    While,
};

use crate::{
    Error, Span, Spanned,
    hir::{Errors, PRINT_DEF_ID},
    value::Int,
};

/// Type-checked high-level intermediate representation.
/// Contains type annotations for all function and variable definitions.
pub struct TypedHir<'a> {
    pub filename: &'a str,
    /// Essentially `HashMap<DefId, DefInfo>`, but all [DefId]`s are consecutive.
    pub defs: Vec<DefInfo>,
    pub funcs: Vec<Func<'a>>,
    pub stmts: Vec<Stmt<'a>>,
}

pub struct DefInfo {
    pub span: Span,
    pub r#type: Type,
}

// TODO: return (Option<TypedHir<'a>>, Vec<Error>) instead because the TypedHir may be recoverable on error
pub fn typed_hir_from_hir<'a>(hir: Hir<'a>) -> Result<TypedHir<'a>, Vec<Error>> {
    let defs = hir
        .defs
        .into_iter()
        .map(|def| {
            DefInfo {
                span: def.span,
                r#type: Type::Nothing, // Default value
            }
        })
        .collect();
    let (errors, defs) = {
        let mut type_checker = TypeChecker {
            errors: Errors::new(),
            defs,
            funcs: &hir.funcs,
            expected_return_type: Type::Nothing,
        };
        type_checker.check_stmts(&hir.stmts);
        type_checker.check_funcs();
        (type_checker.errors, type_checker.defs)
    };
    if !errors.is_empty() {
        return Err(errors.into_inner());
    }
    debug_assert!(defs.iter().all(|def| def.r#type.is_known()));
    Ok(TypedHir {
        filename: hir.filename,
        defs,
        stmts: hir.stmts,
        funcs: hir.funcs,
    })
}

struct TypeChecker<'src, 'funcs> {
    errors: Errors,
    defs: Vec<DefInfo>,
    funcs: &'funcs [Func<'src>],
    expected_return_type: Type,
}

impl<'src, 'funcs> TypeChecker<'src, 'funcs> {
    fn check_funcs(&mut self) {
        for func in self.funcs {
            self.expected_return_type = func
                .return_type
                .as_ref()
                .map(|(r#type, _)| r#type.clone())
                .unwrap_or(Type::Nothing);
            self.defs[func.def_id].r#type = Type::Function;

            for param in &func.params {
                self.defs[param.def_id].r#type = param.r#type.0.clone();
            }
            self.check_stmts(&func.body);
            if func.name.0 == "main" {
                if !func.params.is_empty() {
                    self.errors.add(
                        "The 'main' function may not have any parameters.",
                        func.name.1,
                    );
                } else if func.return_type.is_some() {
                    self.errors
                        .add("The 'main' function may not return anything.", func.name.1);
                }
            }
        }
        self.expected_return_type = Type::Nothing;
    }

    fn check_stmt(&mut self, stmt: &Stmt<'src>) {
        match stmt {
            Stmt::VarDecl(VarDecl {
                def_id,
                name: _,
                value,
            }) => {
                let (r#type, _) = self.check_expr(value);
                self.defs[*def_id].r#type = r#type;
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
                        "The left-hand-side of an assignment must be a variable or list/map element.",
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
                def_id,
                var: _,
                iter,
                body,
            }) => {
                let (iter_type, _) = self.check_expr(iter);
                let var_type = match iter_type {
                    Type::Range => Type::Int,
                    Type::String => Type::Char,
                    Type::List(elem_type) => *elem_type,
                    // TODO: iteration over map (key, value) pairs (requires tuples)
                    _ => {
                        // TODO: add "Did you mean to use a range?" as context if iter_type==Type::Int
                        self.errors
                            .add(format!("Cannot iterate over type '{}'", iter_type), iter.1);
                        Type::UNKNOWN
                    }
                };
                self.defs[*def_id].r#type = var_type;
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
                    .as_ref()
                    .map(|value| self.check_expr(value).0)
                    .unwrap_or(Type::Nothing);
                if value_type != self.expected_return_type {
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

    fn check_stmts(&mut self, stmts: &[Stmt<'src>]) {
        for stmt in stmts {
            self.check_stmt(stmt);
        }
    }

    /// Returns the expression's type and whether it can be assigned to.
    /// Expressions can only be assigned to if they are variables or non-empty lists/maps.
    fn check_expr(&mut self, (expr, span): &Spanned<Expr<'src>>) -> (Type, bool) {
        (
            match expr {
                Expr::Binary(binary) => self.check_binary((binary, *span)),
                Expr::Unary(unary) => self.check_unary((unary, *span)),
                Expr::Call(call) => self.check_call((call, *span)),
                Expr::EmptyList(EmptyList { element_type }) => element_type.0.clone(),
                Expr::List(list) => self.check_list((list, *span)),
                Expr::EmptyMap(EmptyMap {
                    key_type,
                    value_type,
                }) => Type::Map {
                    key: Box::new(key_type.0.clone()),
                    value: Box::new(value_type.0.clone()),
                },
                Expr::Map(map) => self.check_map((map, *span)),
                Expr::Index(index) => {
                    let is_assignable =
                        !matches!(index.expr.0, Expr::EmptyList(_) | Expr::EmptyMap(_));
                    return (self.check_index((index, *span)), is_assignable);
                }
                Expr::Var(def_id) => return (self.defs[*def_id].r#type.clone(), true),
                Expr::Int(int) => {
                    self.check_int((int, *span));
                    Type::Int
                }
                Expr::Bool(_) => Type::Bool,
                Expr::String(_) => Type::String,
                Expr::Range(Range { start, end }) => {
                    self.check_expr(start);
                    self.check_expr(end);
                    Type::Range
                }
            },
            false,
        )
    }

    fn check_binary(&mut self, (binary, span): Spanned<&Binary<'src>>) -> Type {
        let (left_type, _) = self.check_expr(&binary.left);
        let (right_type, _) = self.check_expr(&binary.right);
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

    fn check_unary(&mut self, (unary, span): Spanned<&Unary<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&unary.expr);
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

    fn check_call(&mut self, (call, span): Spanned<&Call<'src>>) -> Type {
        if call.def_id == PRINT_DEF_ID {
            // print(...) accepts any number of arguments of any type.
            return Type::Nothing;
        }
        // TODO: faster lookup
        let func = self
            .funcs
            .iter()
            .find(|func| func.def_id == call.def_id)
            .unwrap();

        let params = &func.params;
        let args = &call.args;
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
        for (arg @ (_, arg_span), param) in std::iter::zip(args, params) {
            let (arg_type, _) = self.check_expr(arg);
            let (param_type, _) = &param.r#type;
            if arg_type.is_known() && arg_type != *param_type {
                let message = format!(
                    "Expected a value of type '{}' but found a value of type '{}'",
                    arg_type, param_type
                );
                self.errors.add(message, *arg_span);
            }
        }
        func.return_type
            .as_ref()
            .map(|(r#type, _)| r#type.clone())
            .unwrap_or(Type::Nothing)
    }

    fn check_list(&mut self, (list, span): Spanned<&List<'src>>) -> Type {
        if list.elements.is_empty() {
            self.errors.add(
                "An empty list must have an element type specified. Example: [<int>]",
                span,
            );
            return Type::UNKNOWN;
        }
        let (first_element_type, _) = self.check_expr(&list.elements[0]);
        for element @ (_, element_span) in &list.elements[1..] {
            let (element_type, _) = self.check_expr(element);
            if types_not_equal(&first_element_type, &element_type) {
                self.errors.add_with_context(
                    format!(
                        "All list elements should have the same type, but this one has type '{}'",
                        element_type
                    ),
                    *element_span,
                    format!("The first element has type '{}'", first_element_type),
                    list.elements[0].1,
                );
            }
        }
        Type::List(Box::new(first_element_type))
    }

    fn check_map(&mut self, (map, span): Spanned<&Map<'src>>) -> Type {
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
        let (first_key_type, _) = self.check_expr(&map.pairs[0].key);
        let (first_value_type, _) = self.check_expr(&map.pairs[0].value);
        for Pair { key, value } in &map.pairs[1..] {
            macro_rules! check {
                ($first_type:expr, $var:ident, $str:literal) => {{
                    let (r#type, _) = self.check_expr($var);
                    if types_not_equal($first_type, &r#type) {
                        self.errors.add_with_context(
                            format!(
                                "All map {}s should have the same type, but this {} has type '{}'",
                                $str, $str, r#type
                            ),
                            $var.1,
                            format!("The first {} has type '{}'", $str, $first_type),
                            map.pairs[0].$var.1,
                        );
                    }
                }};
            }
            check!(&first_key_type, key, "key");
            check!(&first_value_type, value, "value");
        }
        Type::Map {
            key: Box::new(first_key_type),
            value: Box::new(first_value_type),
        }
    }

    fn check_index(&mut self, (index, span): Spanned<&Index<'src>>) -> Type {
        let (expr_type, _) = self.check_expr(&index.expr);
        let (index_type, _) = self.check_expr(&index.index);
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

    fn check_int(&mut self, (int, span): Spanned<&str>) {
        let Err(err) = int.parse::<Int>() else {
            return;
        };
        let message = match err.kind() {
            IntErrorKind::Empty => unreachable!("Empty integers should be impossible."),
            IntErrorKind::Zero => unreachable!("Zeroes should be allowed."),
            IntErrorKind::InvalidDigit => format!(
                "not a decimal digit: '{}'",
                int.chars().find(|c| !c.is_digit(10)).unwrap()
            ),
            IntErrorKind::PosOverflow => {
                format!("integer value may not be greater than {}", Int::MAX)
            }
            IntErrorKind::NegOverflow => {
                format!("integer value may not be smaller than {}", Int::MIN)
            }
            _ => unimplemented!(),
        };
        self.errors.add(message, span);
    }
}

/// Returns `true` if the types are definitively not equal.
///
/// This function also returns `false` if either type is unknown.
fn types_not_equal(left: &Type, right: &Type) -> bool {
    left.is_known() && right.is_known() && left != right
}
