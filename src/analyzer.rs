use std::{collections::HashMap, num::IntErrorKind};

use crate::{Span, Spanned, ast::*};

type Int = i64;

pub struct Error<'a> {
    pub filename: &'a str,
    pub message: String,
    pub span: Span,
}

pub fn analyze<'a>(ast: &'a Ast<'a>, filename: &'a str) -> Vec<Error<'a>> {
    Analyzer::analyze(ast, filename)
}

struct TypedVar<'a> {
    name: Spanned<&'a str>,
    /// [None] on error.
    r#type: Option<Type<'a>>,
}

#[derive(Clone, Copy)]
struct ExprType<'a> {
    r#type: Type<'a>,
    assignable: bool,
}

impl<'a> std::ops::Deref for ExprType<'a> {
    type Target = Type<'a>;

    fn deref(&self) -> &Self::Target {
        &self.r#type
    }
}

impl<'a> From<Type<'a>> for ExprType<'a> {
    fn from(value: Type<'a>) -> Self {
        Self {
            r#type: value,
            assignable: false,
        }
    }
}

#[derive(Default)]
struct Scope<'a> {
    funcs: HashMap<&'a str, &'a Func<'a>>,
    vars: HashMap<&'a str, TypedVar<'a>>,
}

// TODO: Prioritize error messages added first.
//       A type error in a specific piece of code can cause a larger type error
//       in the surrounding code based on guesses by the compiler.

struct Analyzer<'a> {
    filename: &'a str,
    scopes: Vec<Scope<'a>>,
    errors: Vec<Error<'a>>,
}

impl<'a> Analyzer<'a> {
    pub fn analyze(ast: &'a Ast<'a>, filename: &'a str) -> Vec<Error<'a>> {
        let mut analyzer = Self {
            filename,
            scopes: Vec::new(),
            errors: Vec::new(),
        };
        analyzer.analyze_scope(ast);
        analyzer.errors
    }

    fn add_error(&mut self, span: Span, message: impl ToString) {
        self.errors.push(Error {
            filename: self.filename,
            message: message.to_string(),
            span,
        });
    }

    fn in_new_scope(&mut self, f: impl Fn(&mut Self)) {
        self.scopes.push(Scope::default());
        f(self);
        self.scopes.pop();
    }

    fn scope(&self) -> &Scope<'a> {
        self.scopes.last().unwrap()
    }

    fn scope_mut(&mut self) -> &mut Scope<'a> {
        self.scopes.last_mut().unwrap()
    }

    fn get_func(&self, name: &'a str) -> Option<&Func<'a>> {
        self.scope().funcs.get(name).copied()
    }

    fn get_var(&self, name: &'a str) -> Option<&TypedVar<'a>> {
        self.scope().vars.get(name)
    }

    pub fn analyze_scope(&mut self, stmts: &'a [Stmt<'a>]) {
        self.in_new_scope(|s| {
            for stmt in stmts {
                s.analyze_stmt(stmt);
            }
        });
    }

    pub fn analyze_stmt(&mut self, stmt: &'a Stmt<'a>) {
        match stmt {
            Stmt::Func(
                func @ Func {
                    name: (name, name_span),
                    params,
                    return_type,
                    body,
                },
            ) => {
                if self.scope().funcs.contains_key(name) {
                    // TODO: add the other declaration as extra context (see how chumsky does it)
                    self.add_error(
                        *name_span,
                        format!("Function '{name}' is already defined in this scope."),
                    );
                } else {
                    self.scope_mut().funcs.insert(name, func);
                }
                self.in_new_scope(|s| {
                    for param @ Param {
                        name: (name, name_span),
                        r#type,
                    } in params
                    {
                        if s.scope().vars.contains_key(name) {
                            // TODO: add the other declaration as extra context (see how chumsky does it)
                            s.add_error(
                                *name_span,
                                format!("Duplicate parameter name '{name}' in this function declaration."),
                            );
                        } else {
                            s.scope_mut().vars.insert(name, TypedVar {
                                name: param.name,
                                r#type: Some(r#type.0),
                            });
                        }
                        s.analyze_type(r#type);
                    }
                    if let Some(r#type) = return_type {
                        s.analyze_type(r#type);
                    }
                    for (stmt, _) in &body.0 {
                        s.analyze_stmt(stmt);
                    }
                });
            }
            Stmt::VarDecl(
                var @ VarDecl {
                    name: (name, _),
                    value,
                },
            ) => {
                let r#type = self.analyze_expr(value);
                self.scope_mut().vars.insert(
                    name,
                    TypedVar {
                        name: var.name,
                        r#type: r#type.map(|r#type| *r#type),
                    },
                );
            }
            Stmt::Expr(expr) => {
                self.analyze_expr(expr);
            }
            Stmt::Assign(Assign {
                op: (op, op_span),
                target,
                value,
            }) => {
                let target_type = self.analyze_expr(target);
                if target_type.is_some_and(|r#type| !r#type.assignable) {
                    self.add_error(target.1, format!("This expression cannot be assigned to."));
                }
                let value_type = self.analyze_expr(value);
                if let Some((target_type, value_type)) = target_type.zip(value_type) {
                    if *target_type != *value_type {
                        self.add_error(
                            value.1,
                            format!(
                                "Expected a value of type '{}', but found a value of type '{}'",
                                *target_type, *value_type
                            ),
                        );
                    }
                }
                if let Some(target_type) = target_type {
                    use AssignOp::*;
                    match op {
                        Assign => {}
                        Add | Sub | Mul | Div => {
                            if *target_type != Type::Int {
                                // TODO: simplify error message (also in Binary)
                                let message = format!(
                                    "Cannot apply arithmetic operator to values of type '{}'",
                                    *target_type
                                );
                                self.add_error(*op_span, message);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn analyze_expr(&mut self, (expr, span): &Spanned<Expr<'a>>) -> Option<ExprType<'a>> {
        match expr {
            Expr::Binary(Binary {
                op: (op, op_span),
                left,
                right,
            }) => {
                let left_type = self.analyze_expr(left);
                let right_type = self.analyze_expr(right);

                if let Some((left_type, right_type)) = left_type.zip(right_type) {
                    if *left_type != *right_type {
                        self.add_error(
                            *span,
                            format!(
                                "Both sides must be of the same type. Left is '{}' and right is '{}'.",
                                *left_type,
                                *right_type,
                            )
                        );
                    }
                }
                use BinaryOp::*;
                match op {
                    Add | Sub | Div | Mul => {
                        let r#type = left_type.or(right_type);
                        if let Some(r#type) = r#type
                            && *r#type != Type::Int
                        {
                            self.add_error(
                                *op_span,
                                format!(
                                    "Cannot apply arithmetic operator to values of type '{}'",
                                    *r#type
                                ),
                            );
                        }
                        r#type
                    }
                    Eq | NotEq => Some(Type::Bool.into()),
                }
            }
            Expr::Unary(Unary {
                op: (op, op_span),
                expr,
            }) => {
                let expr_type = self.analyze_expr(expr);
                if let Some(expr_type) = expr_type {
                    match op {
                        UnaryOp::Neg => {
                            if *expr_type != Type::Int {
                                self.add_error(
                                    *op_span,
                                    format!(
                                        "Only integers can be negated. Cannot negate '{}'.",
                                        *expr_type
                                    ),
                                );
                            }
                        }
                        UnaryOp::Not => {
                            if *expr_type != Type::Bool {
                                self.add_error(
                                    *op_span,
                                    format!(
                                        "Only booleans can be inverted. Cannot invert '{}'.",
                                        *expr_type
                                    ),
                                );
                            }
                        }
                    }
                }
                expr_type
            }
            Expr::Call(Call {
                name: (name, name_span),
                args,
            }) => {
                // TODO: Find a solution for the lifetime issue so that the Func does not need to be cloned.
                //       Alternatively, make cloning much cheaper by changing the AST to use Rc<Vec<_>>.
                let r#type = match self.get_func(name).cloned() {
                    Some(func) => {
                        if func.params.len() > args.len() {
                            let closing_paren_span = Span {
                                start: span.end - 1,
                                end: span.end,
                                context: span.context,
                            };
                            let message = format!(
                                "This function needs {} more arguments.",
                                func.params.len() - args.len()
                            );
                            self.add_error(closing_paren_span, message);
                        }
                        if func.params.len() < args.len() {
                            let (_, first_arg_after_params_span) = args[func.params.len()];
                            let (_, last_arg_span) = args.last().unwrap();
                            let after_params_span = Span {
                                start: first_arg_after_params_span.start,
                                end: last_arg_span.end,
                                context: last_arg_span.context,
                            };
                            let message = format!(
                                "This function only takes {} arguments. {} arguments passed.",
                                func.params.len(),
                                args.len()
                            );
                            self.add_error(after_params_span, message);
                        }
                        for (arg @ (_, arg_span), param) in std::iter::zip(args, &func.params) {
                            let arg_type = self.analyze_expr(arg);
                            let (param_type, _) = param.r#type;
                            if let Some(arg_type) = arg_type
                                && *arg_type != param_type
                            {
                                let message = format!(
                                    "Expected a value of type '{}' but found a value of type '{}'",
                                    *arg_type, param_type
                                );
                                self.add_error(*arg_span, message);
                            }
                        }
                        Some(
                            func.return_type
                                .map(|(r#type, _)| r#type)
                                .unwrap_or(Type::Nothing),
                        )
                    }
                    // print(...) accepts any number of arguments of any type.
                    None if *name == "print" => Some(Type::Nothing.into()),
                    None => {
                        // TODO: add context if 'name' is defined as variable
                        self.add_error(*name_span, format!("Unknown function '{name}'"));
                        None
                    }
                };
                r#type.map(ExprType::from)
            }
            Expr::Var(name) => match self.get_var(name) {
                Some(var) => var.r#type.map(|r#type| ExprType {
                    r#type,
                    assignable: true,
                }),
                None => {
                    // TODO: add context if 'name' is defined as function
                    self.add_error(*span, format!("Unknown variable '{name}'"));
                    None
                }
            },
            Expr::Int(int) => {
                self.analyze_int((int, *span));
                Some(Type::Int.into())
            }
            Expr::String(_) => Some(Type::String.into()),
        }
    }

    pub fn analyze_type(&mut self, (r#type, span): &Spanned<Type<'a>>) {
        match r#type {
            Type::Named(name) => self.add_error(*span, format!("Unknown type '{name}'")),
            Type::Int | Type::Bool | Type::String | Type::Nothing => {}
        }
    }

    pub fn analyze_int(&mut self, (int, span): Spanned<&'a str>) {
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
            _ => "unknown reason".to_owned(),
        };
        self.add_error(span, message);
    }
}
