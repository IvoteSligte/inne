use std::num::IntErrorKind;

pub use crate::hir::{
    Assign, AssignOp, Binary, BinaryOp, Block, Call, Choice, DefId, Error, Expr, For, Func, Hir,
    If, Loop, Param, Range, Stmt, Type, Unary, UnaryOp, VarDecl, While,
};

pub type Int = i64;

use crate::{
    Span, Spanned,
    hir::{Errors, PRINT_DEF_ID},
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

pub fn typed_hir_from_hir<'a>(hir: Hir<'a>) -> Result<TypedHir<'a>, Vec<Error<'a>>> {
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
            errors: Errors::new(hir.filename),
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

struct TypeChecker<'a, 'b> {
    errors: Errors<'a>,
    defs: Vec<DefInfo>,
    funcs: &'b [Func<'a>],
    expected_return_type: Type,
}

impl<'a, 'b> TypeChecker<'a, 'b> {
    fn check_funcs(&mut self) {
        for func in self.funcs {
            self.expected_return_type = func
                .return_type
                .map(|(r#type, _)| r#type)
                .unwrap_or(Type::Nothing);
            self.defs[func.def_id].r#type = Type::Function;

            for param in &func.params {
                self.defs[param.def_id].r#type = param.r#type.0;
                self.check_type(&param.r#type);
            }
            self.check_stmts(&func.body);
            if let Some(r#type) = func.return_type.as_ref() {
                self.check_type(r#type);
            }
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

    fn check_stmt(&mut self, stmt: &Stmt<'a>) {
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
                        "The left-hand-side of an assignment must be a variable.",
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
                        Add | Sub | Mul | Div => {
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

    fn check_stmts(&mut self, stmts: &[Stmt<'a>]) {
        for stmt in stmts {
            self.check_stmt(stmt);
        }
    }

    /// Returns the expression's type and whether it can be assigned to.
    /// Expressions can only be assigned to if they are variables.
    fn check_expr(&mut self, (expr, span): &Spanned<Expr<'a>>) -> (Type, bool) {
        (
            match expr {
                Expr::Binary(binary) => self.check_binary((binary, *span)),
                Expr::Unary(unary) => self.check_unary((unary, *span)),
                Expr::Call(call) => self.check_call((call, *span)),
                Expr::Var(def_id) => return (self.defs[*def_id].r#type, true),
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

    fn check_binary(&mut self, (binary, span): Spanned<&Binary<'a>>) -> Type {
        let (left_type, _) = self.check_expr(&binary.left);
        let (right_type, _) = self.check_expr(&binary.right);
        let or_type = left_type.or(right_type); // Known if either one is known

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
            Add | Sub | Div | Mul => {
                if or_type.is_known() && or_type != Type::Int {
                    self.errors.add(
                        format!(
                            "Cannot apply arithmetic operator to values of type '{}'",
                            or_type
                        ),
                        binary.op.1,
                    );
                }
                or_type
            }
            And | Or => {
                if or_type.is_known() && or_type != Type::Bool {
                    self.errors.add(
                        format!(
                            "Cannot apply boolean operator to values of type '{}'",
                            or_type
                        ),
                        binary.op.1,
                    );
                }
                or_type
            }
            Eq | NotEq => Type::Bool,
        }
    }

    fn check_unary(&mut self, (unary, span): Spanned<&Unary<'a>>) -> Type {
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

    fn check_call(&mut self, (call, span): Spanned<&Call<'a>>) -> Type {
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
            let (param_type, _) = param.r#type;
            if arg_type.is_known() && arg_type != param_type {
                let message = format!(
                    "Expected a value of type '{}' but found a value of type '{}'",
                    arg_type, param_type
                );
                self.errors.add(message, *arg_span);
            }
        }
        func.return_type
            .map(|(r#type, _)| r#type)
            .unwrap_or(Type::Nothing)
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

    pub fn check_type(&mut self, (r#type, span): &Spanned<Type>) {
        if let Type::Named(name) = r#type {
            self.errors.add(format!("Unknown type '{name}'"), *span);
        }
    }
}
