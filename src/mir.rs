pub use crate::typed_hir::{AssignOp, BinaryOp, DefId, Int, UnaryOp};
use crate::{TypedHir, typed_hir as thir};

pub struct Mir<'a> {
    pub funcs: Vec<Func<'a>>,
    pub blocks: Vec<Block<'a>>,
}

pub type BlockId = usize;

pub struct Func<'a> {
    pub def_id: DefId,
    /// The basic blocks of the function.
    /// This is always at least 1 because the function has a return block.
    pub blocks: Vec<Block<'a>>,
}

/// Basic block
pub struct Block<'a> {
    pub stmts: Vec<Stmt<'a>>,
    pub branch: Branch<'a>,
}

pub enum Branch<'a> {
    Jump { target: BlockId },
    Cond { cond: Expr<'a>, target: BlockId },
    Return(Option<Expr<'a>>),
}

pub enum Stmt<'a> {
    Expr(Expr<'a>),
    VarDecl {
        def_id: DefId,
        value: Expr<'a>,
    },
    /// Emitted when a variable goes out-of-scope.
    VarDrop(DefId),
    Assign {
        target: DefId,
        value: Expr<'a>,
    },
}

pub enum Expr<'a> {
    Binary {
        op: BinaryOp,
        left: Box<Expr<'a>>,
        right: Box<Expr<'a>>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr<'a>>,
    },
    Call {
        def_id: DefId,
        args: Vec<Expr<'a>>,
    },
    Var(DefId),
    Int(Int),
    String(&'a str),
}

pub struct Var<'a> {
    pub name: &'a str,
    pub def_id: DefId,
}

pub fn mir_from_typed_hir<'a>(typed_hir: &'a TypedHir<'a>) -> Mir<'a> {
    Constructor::map_thir(typed_hir)
}

struct Constructor {}

impl Constructor {
    fn map_thir<'a>(typed_hir: &'a TypedHir<'a>) -> Mir<'a> {
        let mut _self = Self {};
        let funcs = typed_hir
            .funcs
            .iter()
            .map(|func| _self.map_func(func))
            .collect();
        let mut blocks = Vec::new();
        _self.map_scope(&typed_hir.stmts, Branch::Return(None), &mut blocks);
        Mir { funcs, blocks }
    }

    fn map_func<'a>(&mut self, func: &'a thir::Func<'a>) -> Func<'a> {
        let mut blocks = Vec::new();
        // TODO: allocate params? or is that handled by the caller?
        self.map_scope(&func.body, Branch::Return(None), &mut blocks);
        Func {
            def_id: func.def_id,
            blocks,
        }
    }

    fn map_scope<'a>(
        &mut self,
        stmts: &'a [thir::Stmt<'a>],
        end_branch: Branch<'a>,
        dst: &mut Vec<Block<'a>>,
    ) {
        let mut block_stmts = Vec::new();
        let mut declared_vars = Vec::new();

        for stmt in stmts {
            match stmt {
                thir::Stmt::VarDecl(thir::VarDecl {
                    def_id,
                    name: _,
                    value,
                }) => {
                    block_stmts.push(Stmt::VarDecl {
                        def_id: *def_id,
                        value: self.map_expr(&value.0),
                    });
                    declared_vars.push(*def_id);
                }
                thir::Stmt::Expr(expr) => {
                    block_stmts.push(Stmt::Expr(self.map_expr(&expr.0)));
                }
                thir::Stmt::Assign(thir::Assign { op, target, value }) => {
                    let target = match target.0 {
                        thir::Expr::Var(def_id) => def_id,
                        _ => unreachable!(),
                    };
                    use AssignOp::*;
                    let mut value = self.map_expr(&value.0);
                    if op.0 != Assign {
                        // Rewrite statements such as `x += 1` to `x = x + 1`
                        value = Expr::Binary {
                            op: match op.0 {
                                Add => BinaryOp::Add,
                                Sub => BinaryOp::Sub,
                                Mul => BinaryOp::Mul,
                                Div => BinaryOp::Div,
                                Assign => unreachable!(),
                            },
                            // NOTE: currently the only target is a Var,
                            //       but in the future care will need to be taken to prevent
                            //       the target from being evaluated twice.
                            left: Box::new(Expr::Var(target)),
                            right: Box::new(value),
                        };
                    }
                    block_stmts.push(Stmt::Assign { target, value });
                }
            }
        }
        for var in declared_vars {
            // TODO: drop var as soon as it is no longer used instead of at end-of-scope
            block_stmts.push(Stmt::VarDrop(var));
        }
        if !block_stmts.is_empty() {
            dst.push(Block {
                stmts: block_stmts,
                branch: end_branch,
            });
        }
    }

    fn map_expr<'a>(&mut self, expr: &'a thir::Expr<'a>) -> Expr<'a> {
        match expr {
            thir::Expr::Binary(thir::Binary { op, left, right }) => Expr::Binary {
                op: op.0,
                left: Box::new(self.map_expr(&left.0)),
                right: Box::new(self.map_expr(&right.0)),
            },
            thir::Expr::Unary(thir::Unary { op, expr }) => Expr::Unary {
                op: op.0,
                expr: Box::new(self.map_expr(&expr.0)),
            },
            thir::Expr::Call(thir::Call {
                name: _,
                def_id,
                args,
            }) => Expr::Call {
                def_id: *def_id,
                args: args.iter().map(|arg| self.map_expr(&arg.0)).collect(),
            },
            // TODO: make sure to add AddDependency statements to keep objects alive
            //       that are referenced by variables in the expression
            thir::Expr::Var(def_id) => Expr::Var(*def_id),
            thir::Expr::Int(int) => Expr::Int(int.parse().unwrap()),
            thir::Expr::String(string) => Expr::String(string),
        }
    }
}
