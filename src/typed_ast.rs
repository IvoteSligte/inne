use std::{collections::HashMap, rc::Rc};

pub use crate::ast::{AssignOp, BinaryOp, UnaryOp};

use crate::{Ast, Span, Spanned, ast};

pub struct TypedAst<'a> {
    funcs: Vec<Rc<Func<'a>>>,
    stmts: Vec<Stmt<'a>>,
}

pub enum Stmt<'a> {
    VarDecl(Rc<VarDecl<'a>>),
    Expr(Spanned<Expr<'a>>),
    Assign(Assign<'a>),
}

pub type Block<'a> = Vec<Stmt<'a>>;

pub struct Func<'a> {
    pub name: Spanned<&'a str>,
    pub params: Vec<Rc<Param<'a>>>,
    pub return_type: Option<Spanned<Type>>,
    pub body: Block<'a>,
}

pub struct Param<'a> {
    pub name: Spanned<&'a str>,
    pub r#type: Spanned<Type>,
}

pub struct VarDecl<'a> {
    pub name: Spanned<&'a str>,
    pub value: Spanned<Expr<'a>>,
    /// Inferred type.
    pub r#type: Type,
}

pub enum Type {
    Nothing,
    Int,
    Bool,
    String,
    // Struct(Rc<Struct<'a>>)
    // Enum(Rc<Enum<'a>>)
}

pub enum Expr<'a> {
    Binary(Binary<'a>),
    Unary(Unary<'a>),
    Call(Call<'a>),
    Var(Var<'a>),
    Int(&'a str),
    String(&'a str),
}

pub struct Binary<'a> {
    pub op: Spanned<BinaryOp>,
    pub left: Box<Spanned<Expr<'a>>>,
    pub right: Box<Spanned<Expr<'a>>>,
}

pub struct Unary<'a> {
    pub op: Spanned<UnaryOp>,
    pub expr: Box<Spanned<Expr<'a>>>,
}

pub struct Call<'a> {
    pub func: Rc<Func<'a>>,
    pub name: Spanned<&'a str>,
    pub args: Vec<Spanned<Expr<'a>>>,
}

#[derive(Clone)]
pub enum VarOrParam<'a> {
    Var(Rc<VarDecl<'a>>),
    Param(Rc<Param<'a>>),
}

pub struct Var<'a> {
    pub decl: VarOrParam<'a>,
    pub name: &'a str,
}

pub struct Assign<'a> {
    pub op: Spanned<AssignOp>,
    pub target: Spanned<Expr<'a>>,
    pub value: Spanned<Expr<'a>>,
}

pub struct Error<'a> {
    pub filename: &'a str,
    pub message: String,
    pub span: Span,
    pub contexts: Vec<(String, Span)>,
}

pub fn typed_ast_from_ast<'a>(
    ast: &'a Ast<'a>,
    filename: &'a str,
) -> (TypedAst<'a>, Vec<Error<'a>>) {
    let mut errors = Vec::new();
    let mut funcs = HashMap::<&'a str, (Rc<Func<'a>>, &ast::Func<'a>)>::new();
    let mut add_error = |message, span, contexts| {
        errors.push(Error {
            filename,
            message,
            span,
            contexts,
        });
    };
    // 1. Collect top-level functions.

    // 2.
    //    a. Link variables, types, and function calls to their declarations.
    //    b. Add types to expressions.

    let mut scopes = Vec::<HashMap<&str, VarOrParam>>::new();

    todo!();
}

struct Constructor<'a> {
    filename: &'a str,
    funcs: HashMap<&'a str, (Rc<Func<'a>>, &'a ast::Func<'a>)>,
    scopes: Vec<HashMap<&'a str, VarOrParam<'a>>>,
    errors: Vec<Error<'a>>,
}

impl<'a> Constructor<'a> {
    pub fn map_ast(
        ast: &'a ast::Ast<'a>,
        filename: &'a str,
    ) -> Result<TypedAst<'a>, Vec<Error<'a>>> {
        let mut constructor = Self {
            filename,
            funcs: HashMap::new(),
            scopes: Vec::new(),
            errors: Vec::new(),
        };
        constructor.extract_funcs(ast);
        let stmts = ast
            .iter()
            .filter_map(|stmt| constructor.map_stmt(stmt))
            .collect();
        if constructor.errors.is_empty() {
            Ok(TypedAst {
                funcs: constructor
                    .funcs
                    .into_values()
                    .map(|(func, _)| func)
                    .collect(),
                stmts,
            })
        } else {
            Err(constructor.errors)
        }
    }

    fn extract_funcs(&mut self, ast: &'a ast::Ast<'a>) {
        debug_assert!(self.funcs.is_empty());
        for stmt in ast {
            let ast::Stmt::Func(func) = stmt else {
                continue;
            };
            let (name, name_span) = func.name;
            if let Some((prev_func, _)) = self.funcs.get(name) {
                self.add_error_with_contexts(
                    format!(
                        "Function '{}' was already defined before. Functions must have unique names.",
                        name
                    ),
                    name_span,
                    vec![("Previously defined here:".to_string(), prev_func.name.1)],
                );
            } else {
                self.funcs.insert(
                    name,
                    (
                        Rc::new(Func {
                            name: func.name,
                            params: Default::default(),
                            return_type: Default::default(),
                            body: Default::default(),
                        }),
                        func,
                    ),
                );
            }
        }
    }

    fn add_error(&mut self, message: impl ToString, span: Span) {
        self.add_error_with_contexts(message, span, Vec::new());
    }

    fn add_error_with_contexts(
        &mut self,
        message: impl ToString,
        span: Span,
        contexts: Vec<(String, Span)>,
    ) {
        self.errors.push(Error {
            filename: self.filename,
            message: message.to_string(),
            span,
            contexts,
        });
    }

    fn map_vec<T, U>(&mut self, vec: &[T], f: impl Fn(&mut Self, &T) -> U) -> Vec<U> {
        vec.iter().map(|item| f(self, item)).collect()
    }

    fn map_func(&mut self, func: &ast::Func<'a>, dst: &mut Func<'a>) {
        todo!()
    }

    /// Returns [None] on error. Assumes that the given statement is not a function.
    fn map_stmt<'b>(&mut self, stmt: &'b ast::Stmt<'b>) -> Option<Stmt<'b>>
    where
        'a: 'b,
    {
        match stmt {
            ast::Stmt::VarDecl(var_decl) => {
                let value = self.map_expr(&var_decl.value)?;
                let new_var_decl = Rc::new(VarDecl {
                    name: var_decl.name,
                    value: (value, var_decl.value.1),
                    r#type: ,
                });
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(var_decl.name.0, VarOrParam::Var(new_var_decl.clone()));
                Some(new_var_decl)
            }
            ast::Stmt::Expr(expr @ (_, span)) => {
                self.map_expr(expr).map(|expr| Stmt::Expr((expr, *span)))
            }
            ast::Stmt::Assign(assign) => {
                todo!()
            }
            ast::Stmt::Func(_) => unreachable!("Functions are mapped separately."),
        }
    }

    /// Returns [None] on error.
    fn map_expr<'b>(&mut self, (expr, span): &'b Spanned<ast::Expr<'b>>) -> Option<Expr<'b>>
    where
        'a: 'b,
    {
        match expr {
            ast::Expr::Binary(ast::Binary { op, left, right }) => self
                .map_expr(left)
                .zip(self.map_expr(right))
                .map(|(new_left, new_right)| {
                    Expr::Binary(Binary {
                        op: *op,
                        left: Box::new((new_left, left.1)),
                        right: Box::new((new_right, right.1)),
                    })
                }),
            ast::Expr::Unary(ast::Unary { op, expr }) => self.map_expr(expr).map(|new_expr| {
                Expr::Unary(Unary {
                    op: *op,
                    expr: Box::new((new_expr, expr.1)),
                })
            }),
            ast::Expr::Call(call) => {
                let (name, name_span) = call.name;
                let func = self.funcs.get(name).map(|(func, _)| func.clone());
                if func.is_none() {
                    // TODO: add similarly named functions as contexts
                    self.add_error(format!("Unknown function '{}'", name), name_span);
                }
                func.and_then(|func| {
                    let args = call
                        .args
                        .iter()
                        .filter_map(|arg @ (_, span)| match self.map_expr(arg) {
                            Some(expr) => Some((expr, *span)),
                            None => None,
                        })
                        .collect::<Vec<_>>();
                    (args.len() == call.args.len()).then_some(Expr::Call(Call {
                        func,
                        name: call.name,
                        args,
                    }))
                })
            }
            ast::Expr::Var(name) => {
                match self.scopes.iter().rev().find_map(|scope| scope.get(name)) {
                    Some(decl) => Some(Expr::Var(Var {
                        decl: decl.clone(),
                        name,
                    })),
                    None => {
                        self.add_error(format!("Unknown variable '{name}'"), *span);
                        None
                    }
                }
            }
            ast::Expr::Int(int) => Some(Expr::Int(int)),
            ast::Expr::String(string) => Some(Expr::String(string)),
        }
    }
}
