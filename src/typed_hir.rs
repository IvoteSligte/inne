use std::num::IntErrorKind;

pub use crate::hir::{
    Assign, AssignOp, Binary, BinaryOp, Block, Call, DefId, Error, Expr, Func, Hir, Param, Stmt,
    Type, Unary, UnaryOp, Var, VarDecl,
};

pub type Int = i64;

use crate::{
    Span, Spanned,
    hir::{Errors, PRINT_DEF_ID},
};

/// Type-checked high-level intermediate representation.
/// Contains type annotations for all function and variable definitions.
pub struct TypedHir<'a> {
    /// Essentially `HashMap<DefId, DefInfo>`, but all [DefId]`s are consecutive.
    pub defs: Vec<DefInfo>,
    pub funcs: Vec<Func<'a>>,
    pub stmts: Vec<Stmt<'a>>,
}

pub struct DefInfo {
    pub span: Span,
    pub r#type: Type,
}

pub fn typed_hir_from_hir<'a>(
    hir: Hir<'a>,
    filename: &'a str,
) -> Result<TypedHir<'a>, Vec<Error<'a>>> {
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

    let mut type_checker = TypeChecker {
        errors: Errors::new(filename),
        defs,
    };
    type_checker.check_funcs(&hir.funcs);
    for stmt in &hir.stmts {
        type_checker.check_stmt(&hir.funcs, stmt);
    }
    match type_checker.errors.is_empty() {
        true => {
            debug_assert!(type_checker.defs.iter().all(|def| def.r#type.is_known()));
            Ok(TypedHir {
                defs: type_checker.defs,
                funcs: hir.funcs,
                stmts: hir.stmts,
            })
        }
        false => Err(type_checker.errors.into_inner()),
    }
}

struct TypeChecker<'a> {
    errors: Errors<'a>,
    defs: Vec<DefInfo>,
}

impl<'a> TypeChecker<'a> {
    fn check_funcs(&mut self, funcs: &[Func<'a>]) {
        for func in funcs {
            self.defs[func.def_id].r#type = Type::Function;

            for param in &func.params {
                self.defs[param.def_id].r#type = param.r#type.0;
                self.check_type(&param.r#type);
            }
            for stmt in &func.body {
                self.check_stmt(funcs, stmt);
            }
            if let Some(r#type) = func.return_type.as_ref() {
                self.check_type(r#type);
            }
        }
    }

    fn check_stmt(&mut self, funcs: &[Func<'a>], stmt: &Stmt<'_>) {
        match stmt {
            Stmt::VarDecl(VarDecl {
                def_id,
                name: _,
                value,
            }) => {
                let (r#type, _) = self.check_expr(funcs, value);
                self.defs[*def_id].r#type = r#type;
            }
            Stmt::Expr(expr) => {
                self.check_expr(funcs, expr);
            }
            Stmt::Assign(Assign {
                op: (op, op_span),
                target,
                value,
            }) => {
                let (target_type, target_is_assignable) = self.check_expr(funcs, target);
                let (value_type, _) = self.check_expr(funcs, value);
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
        }
    }

    /// Returns the expression's type and whether it can be assigned to.
    /// Expressions can only be assigned to if they are variables.
    fn check_expr(&mut self, funcs: &[Func<'a>], (expr, span): &Spanned<Expr<'_>>) -> (Type, bool) {
        match expr {
            Expr::Binary(binary) => (self.check_binary(funcs, (binary, *span)), false),
            Expr::Unary(unary) => (self.check_unary(funcs, (unary, *span)), false),
            Expr::Call(call) => (self.check_call(funcs, (call, *span)), false),
            Expr::Var(def_id) => (self.defs[*def_id].r#type, true),
            Expr::Int(int) => {
                self.check_int((int, *span));
                (Type::Int, false)
            }
            Expr::String(_) => (Type::String, false),
        }
    }

    fn check_binary(&mut self, funcs: &[Func<'a>], (binary, span): Spanned<&Binary<'_>>) -> Type {
        let (left_type, _) = self.check_expr(funcs, &binary.left);
        let (right_type, _) = self.check_expr(funcs, &binary.right);
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
            Eq | NotEq => Type::Bool,
        }
    }

    fn check_unary(&mut self, funcs: &[Func<'a>], (unary, span): Spanned<&Unary<'_>>) -> Type {
        let (expr_type, _) = self.check_expr(funcs, &unary.expr);
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

    fn check_call(&mut self, funcs: &[Func<'a>], (call, span): Spanned<&Call<'_>>) -> Type {
        if call.def_id == PRINT_DEF_ID {
            // print(...) accepts any number of arguments of any type.
            return Type::Nothing;
        }
        // TODO: faster lookup
        let func = funcs
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
            let (arg_type, _) = self.check_expr(funcs, arg);
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
        match r#type {
            Type::Named(name) => self.errors.add(format!("Unknown type '{name}'"), *span),
            Type::Int | Type::Bool | Type::String | Type::Nothing | Type::Function => {}
        }
    }
}
