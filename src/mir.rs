use std::collections::HashMap;

pub use crate::typed_hir::{AssignOp, BinaryOp, DefId, Int, UnaryOp};
use crate::{TypedHir, typed_hir as thir};

pub struct Mir<'a> {
    pub funcs: Vec<Func<'a>>,
    pub blocks: Vec<Block<'a>>,
}

/// ID of a basic block.
#[derive(Debug, Clone, Copy)]
pub struct BlockId(usize);

impl BlockId {
    pub fn get(self) -> usize {
        self.0
    }
}

/// ID of a local variable (including parameters and return value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalId(usize);

impl LocalId {
    pub const RETURN_VALUE: Self = Self(0);

    pub fn get(self) -> usize {
        self.0
    }
}

pub struct Func<'a> {
    pub def_id: DefId,
    /// The basic blocks of the function.
    /// This is always at least 1 because the function has a return block.
    pub blocks: Vec<Block<'a>>,
}

/// A basic block.
pub struct Block<'a> {
    pub stmts: Vec<Stmt<'a>>,
    pub branch: Branch,
}

pub enum Branch {
    Jump { target: BlockId },
    Cond { cond: LocalId, target: BlockId },
    Return(Option<LocalId>),
}

pub enum Op {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
    Neg,
    Not,
    Int,
    String,
    Call,
}

pub struct Stmt<'a> {
    pub op: Op,
    pub args: Vec<LocalId>,

    // Constants (mutually exclusive and optional)
    pub func_id: DefId,
    pub int: Int,
    pub string: &'a str,
}

pub fn mir_from_typed_hir<'a>(typed_hir: &'a TypedHir<'a>) -> Mir<'a> {
    Constructor::map_thir(typed_hir)
}

struct FuncBuilder<'a> {
    def_id: DefId,
    /// Total number of statements in the function.
    num_stmts: usize,
    locals: HashMap<DefId, LocalId>,
    blocks: Vec<Block<'a>>,
    /// Statements for the current block.
    stmts: Vec<Stmt<'a>>,
}

impl<'a> FuncBuilder<'a> {
    pub fn new(def_id: DefId) -> Self {
        Self {
            def_id,
            locals: HashMap::new(),
            num_stmts: 0,
            blocks: Vec::new(),
            stmts: Vec::new(),
        }
    }

    pub fn declare(&mut self, def_id: DefId, local_id: LocalId) {
        self.locals.insert(def_id, local_id);
    }

    pub fn get_local(&self, def_id: DefId) -> LocalId {
        *self.locals.get(&def_id).unwrap()
    }

    pub fn push_stmt(&mut self, op: Op, args: Vec<LocalId>) -> LocalId {
        self.stmts.push(Stmt {
            op,
            args,
            int: 0,
            string: "",
            func_id: 0,
        });
        // Equal to 1 + current_stmt because the return slot is LocalId(0)
        // NOTE: does this need to take params into account?
        LocalId(self.num_stmts)
    }

    pub fn push_int(&mut self, int: Int) -> LocalId {
        self.stmts.push(Stmt {
            op: Op::Int,
            args: Vec::new(),
            int,
            string: "",
            func_id: 0,
        });
        // Equal to 1 + current_stmt because the return slot is LocalId(0)
        // NOTE: does this need to take params into account?
        LocalId(self.num_stmts)
    }

    pub fn push_string(&mut self, string: &'a str) -> LocalId {
        self.stmts.push(Stmt {
            op: Op::String,
            args: Vec::new(),
            int: 0,
            string,
            func_id: 0,
        });
        // Equal to 1 + current_stmt because the return slot is LocalId(0)
        // NOTE: does this need to take params into account?
        LocalId(self.num_stmts)
    }

    pub fn push_call(&mut self, func_id: DefId, args: Vec<LocalId>) -> LocalId {
        self.stmts.push(Stmt {
            op: Op::Call,
            args,
            int: 0,
            string: "",
            func_id,
        });
        // Equal to 1 + current_stmt because the return slot is LocalId(0)
        // NOTE: does this need to take params into account?
        LocalId(self.num_stmts)
    }

    pub fn block_is_empty(&self) -> bool {
        self.stmts.is_empty()
    }

    pub fn finish_block(&mut self, branch: Branch) {
        self.blocks.push(Block {
            stmts: std::mem::take(&mut self.stmts),
            branch,
        });
    }

    pub fn build(self) -> Func<'a> {
        debug_assert!(self.stmts.is_empty());
        Func {
            def_id: self.def_id,
            blocks: self.blocks,
        }
    }
}

struct Constructor {}

impl Constructor {
    pub fn map_thir<'a>(typed_hir: &'a TypedHir<'a>) -> Mir<'a> {
        let mut _self = Self {};
        let funcs = typed_hir
            .funcs
            .iter()
            .map(|func| _self.map_func(func))
            .collect();
        let blocks = {
            let mut builder = FuncBuilder::new(DefId::MAX);
            _self.map_scope(&typed_hir.stmts, Branch::Return(None), &mut builder);
            builder.build().blocks
        };
        Mir { funcs, blocks }
    }

    fn map_func<'a>(&mut self, func: &'a thir::Func<'a>) -> Func<'a> {
        let mut builder = FuncBuilder::new(func.def_id);
        // TODO: allocate params? or is that handled by the caller?
        self.map_scope(&func.body, Branch::Return(None), &mut builder);
        builder.build()
    }

    fn map_scope<'a>(
        &mut self,
        stmts: &'a [thir::Stmt<'a>],
        end_branch: Branch,
        builder: &mut FuncBuilder<'a>,
    ) {
        for stmt in stmts {
            match stmt {
                thir::Stmt::VarDecl(thir::VarDecl {
                    def_id,
                    name: _,
                    value,
                }) => {
                    let value_id = self.map_expr(&value.0, builder);
                    builder.declare(*def_id, value_id);
                }
                thir::Stmt::Expr(expr) => {
                    self.map_expr(&expr.0, builder);
                }
                thir::Stmt::Assign(thir::Assign { op, target, value }) => {
                    let target_id = match target.0 {
                        thir::Expr::Var(def_id) => builder.get_local(def_id),
                        _ => unreachable!(),
                    };
                    let mut value_id = self.map_expr(&value.0, builder);

                    use AssignOp::*;
                    if op.0 != Assign {
                        // Rewrite statements such as `x += 1` to `x = x + 1`
                        let op = match op.0 {
                            Add => Op::Add,
                            Sub => Op::Sub,
                            Mul => Op::Mul,
                            Div => Op::Div,
                            Assign => unreachable!(),
                        };
                        value_id = builder.push_stmt(op, vec![target_id, value_id]);
                    }
                    builder.push_stmt(Op::Assign, vec![target_id, value_id]);
                }
            }
        }
        // TODO: drop declared variables that hold pointers

        if !builder.block_is_empty() {
            builder.finish_block(end_branch);
        }
    }

    fn map_expr<'a>(&mut self, expr: &'a thir::Expr<'a>, builder: &mut FuncBuilder<'a>) -> LocalId {
        match expr {
            thir::Expr::Binary(thir::Binary { op, left, right }) => {
                let left_id = self.map_expr(&left.0, builder);
                let right_id = self.map_expr(&right.0, builder);
                let op = match op.0 {
                    BinaryOp::Add => Op::Add,
                    BinaryOp::Sub => Op::Sub,
                    BinaryOp::Mul => Op::Mul,
                    BinaryOp::Div => Op::Div,
                    BinaryOp::Eq => Op::Eq,
                    BinaryOp::NotEq => Op::NotEq,
                };
                builder.push_stmt(op, vec![left_id, right_id])
            }
            thir::Expr::Unary(thir::Unary { op, expr }) => {
                let expr_id = self.map_expr(&expr.0, builder);
                let op = match op.0 {
                    UnaryOp::Neg => Op::Neg,
                    UnaryOp::Not => Op::Not,
                };
                builder.push_stmt(op, vec![expr_id])
            }
            thir::Expr::Call(thir::Call {
                name: _,
                def_id,
                args,
            }) => {
                let arg_ids = args
                    .iter()
                    .map(|arg| self.map_expr(&arg.0, builder))
                    .collect();
                builder.push_call(*def_id, arg_ids)
            }
            thir::Expr::Var(def_id) => builder.get_local(*def_id),
            thir::Expr::Int(int) => builder.push_int(int.parse().unwrap()),
            thir::Expr::String(string) => builder.push_string(string),
        }
    }
}
