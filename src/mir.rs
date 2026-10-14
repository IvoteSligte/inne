use std::collections::HashMap;

pub use crate::typed_hir::{AssignOp, BinaryOp, DefId, Int, UnaryOp};
use crate::{TypedHir, hir::PRINT_DEF_ID, typed_hir as thir};

pub struct Mir<'a> {
    pub funcs: Vec<Func<'a>>,
    pub stack_space: usize,
    pub blocks: Vec<Block<'a>>,
}

macro_rules! usize_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name(usize);

        impl $name {
            pub fn get(self) -> usize {
                self.0
            }
        }
    };
}

usize_newtype!(FuncId);
usize_newtype!(BlockId);
usize_newtype!(LocalId);

impl FuncId {
    pub const DEFAULT: Self = Self(usize::MAX);
}

impl LocalId {
    pub const RETURN_VALUE: Self = Self(0);
}

pub struct Func<'a> {
    pub name: &'a str,
    pub stack_space: usize,
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

#[derive(Debug, Clone, Copy)]
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
    Print,
}

pub struct Stmt<'a> {
    pub local_id: LocalId,
    pub op: Op,
    pub args: Vec<LocalId>,

    // Constants (mutually exclusive and optional)
    pub func_id: FuncId,
    pub int: Int,
    pub string: &'a str,
}

pub fn mir_from_typed_hir<'a>(typed_hir: &'a TypedHir<'a>) -> Mir<'a> {
    Constructor::map_thir(typed_hir)
}

struct FuncBuilder<'a> {
    /// Function name.
    name: &'a str,
    /// Maps [DefId]s for variable declarations to their [LocalId].
    def_to_local: HashMap<DefId, LocalId>,
    /// The number of parameters this function has.
    num_params: usize,
    /// Counter for the number of locals in this function.
    num_locals: usize,
    blocks: Vec<Block<'a>>,
    /// Statements for the current block.
    stmts: Vec<Stmt<'a>>,
}

impl<'a> FuncBuilder<'a> {
    pub fn new(name: &'a str, num_params: usize) -> Self {
        Self {
            name,
            def_to_local: HashMap::new(),
            num_params,
            num_locals: 0,
            blocks: Vec::new(),
            stmts: Vec::new(),
        }
    }

    pub fn declare(&mut self, def_id: DefId, local_id: LocalId) {
        self.def_to_local.insert(def_id, local_id);
    }

    pub fn get_local(&self, def_id: DefId) -> LocalId {
        *self.def_to_local.get(&def_id).unwrap()
    }

    fn new_local(&mut self) -> LocalId {
        self.num_locals += 1;
        // Equal to 1 + num_params + num_locals_before_this_function
        // because the return value has Localid(0).
        LocalId(self.num_params + self.num_locals)
    }

    fn push_stmt(
        &mut self,
        op: Op,
        args: Vec<LocalId>,
        int: Int,
        string: &'a str,
        func_id: FuncId,
    ) -> LocalId {
        let local_id = self.new_local();
        self.stmts.push(Stmt {
            local_id,
            op,
            args,
            func_id,
            int,
            string,
        });
        local_id
    }

    pub fn push_op(&mut self, op: Op, args: Vec<LocalId>) -> LocalId {
        self.push_stmt(op, args, 0, "", FuncId::DEFAULT)
    }

    pub fn push_int(&mut self, int: Int) -> LocalId {
        self.push_stmt(Op::Int, Vec::new(), int, "", FuncId::DEFAULT)
    }

    pub fn push_string(&mut self, string: &'a str) -> LocalId {
        self.push_stmt(Op::String, Vec::new(), 0, string, FuncId::DEFAULT)
    }

    pub fn push_call(&mut self, func_id: FuncId, args: Vec<LocalId>) -> LocalId {
        self.push_stmt(Op::Call, args, 0, "", func_id)
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
            name: self.name,
            // Return value + number of params + number of locals
            stack_space: 1 + self.num_params + self.num_locals,
            blocks: self.blocks,
        }
    }
}

struct Constructor {
    def_to_func: HashMap<DefId, FuncId>,
}

impl Constructor {
    pub fn map_thir<'a>(typed_hir: &'a TypedHir<'a>) -> Mir<'a> {
        let mut _self = Self {
            def_to_func: typed_hir
                .funcs
                .iter()
                .enumerate()
                .map(|(i, func)| (func.def_id, FuncId(i)))
                .collect(),
        };
        let funcs = typed_hir
            .funcs
            .iter()
            .map(|func| _self.map_func(func))
            .collect();
        let mut builder = FuncBuilder::new("%main", 0);
        _self.map_scope(&typed_hir.stmts, Branch::Return(None), &mut builder);
        let main_func = builder.build();

        Mir {
            funcs,
            stack_space: main_func.stack_space,
            blocks: main_func.blocks,
        }
    }

    fn map_func<'a>(&mut self, func: &'a thir::Func<'a>) -> Func<'a> {
        let mut builder = FuncBuilder::new(func.name.0, func.params.len());
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
                        value_id = builder.push_op(op, vec![target_id, value_id]);
                    }
                    builder.push_op(Op::Assign, vec![target_id, value_id]);
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
                builder.push_op(op, vec![left_id, right_id])
            }
            thir::Expr::Unary(thir::Unary { op, expr }) => {
                let expr_id = self.map_expr(&expr.0, builder);
                let op = match op.0 {
                    UnaryOp::Neg => Op::Neg,
                    UnaryOp::Not => Op::Not,
                };
                builder.push_op(op, vec![expr_id])
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
                if *def_id == PRINT_DEF_ID {
                    builder.push_op(Op::Print, arg_ids)
                } else {
                    let func_id = *self.def_to_func.get(def_id).unwrap();
                    builder.push_call(func_id, arg_ids)
                }
            }
            thir::Expr::Var(def_id) => builder.get_local(*def_id),
            thir::Expr::Int(int) => builder.push_int(int.parse().unwrap()),
            thir::Expr::String(string) => builder.push_string(string),
        }
    }
}

use indentify::*;

impl std::fmt::Display for Mir<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NOTE: Not sure why it doesn't work without the dyn conversion.
        let f: &mut dyn std::fmt::Write = &mut IndentWriter::new("    ", f);
        writeln!(f, "MIR:")?;
        for func in &self.funcs {
            writeln!(f, "{func}")?;
        }
        for (i, block) in self.blocks.iter().enumerate() {
            writeln!(f, "block {i}: {block}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Func<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            name,
            stack_space,
            blocks,
        } = self;
        writeln!(f, "func {name}({} locals) {{{INDENT}", stack_space - 1)?;
        for (i, block) in blocks.iter().enumerate() {
            writeln!(f, "block {i}: {block}")?;
        }
        write!(f, "{DEDENT}}}")?;
        Ok(())
    }
}

impl std::fmt::Display for Block<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { stmts, branch } = self;
        writeln!(f, "{{{INDENT}")?;
        for stmt in stmts {
            writeln!(f, "{stmt}")?;
        }
        writeln!(f, "{branch}")?;
        write!(f, "{DEDENT}}}")?;
        Ok(())
    }
}

impl std::fmt::Display for Stmt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            local_id,
            op,
            args,
            func_id,
            int,
            string,
        } = self;
        write!(f, "var _{} = {op:?}", local_id.get() - 1)?;
        match op {
            Op::Int => write!(f, "<{}>", int)?,
            Op::String => write!(f, "<{:?}>", string)?,
            Op::Call => write!(f, "<{:?}>", func_id.get())?,
            _ => {}
        }
        write!(f, "(",)?;
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "_{}", arg.get() - 1)?;
        }
        write!(f, ")")?;
        Ok(())
    }
}

impl std::fmt::Display for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Branch::Jump { target } => write!(f, "Jump(block {})", target.get()),
            Branch::Cond { cond, target } => {
                write!(f, "Cond({}, block {})", cond.get(), target.get())
            }
            Branch::Return(local_id) => {
                write!(f, "Return")?;
                match local_id {
                    Some(local_id) => write!(f, "(_{})", local_id.get() - 1),
                    None => Ok(()),
                }
            }
        }
    }
}
