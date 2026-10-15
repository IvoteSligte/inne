use std::collections::HashMap;

pub use crate::typed_hir::{DefId, DefInfo, Error, Int};
use crate::{
    TypedHir,
    hir::{Errors, PRINT_DEF_ID},
    typed_hir as thir,
};

// TODO: reachability analysis
//       - check that non-void functions always return
//       - warn for unused code
//       - check for tautological conditions

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String(Vec<char>),
    Range { start: Int, end: Int },
}

pub struct Mir<'a> {
    pub funcs: Vec<Func<'a>>,
    pub stack_space: usize,
    pub blocks: Vec<Block>,
}

macro_rules! usize_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub blocks: Vec<Block>,
}

/// A basic block.
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub branch: Branch,
}

pub enum Branch {
    Jump(BlockId),
    Cond { cond: LocalId, target: BlockId },
    Return,
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
    Constant,
    Call,
    Print,
    Range,
    IntoIter,
    Next,
}

// TODO: just add the args to Op, making it a tagged union
pub struct Stmt {
    pub local_id: LocalId,
    pub op: Op,
    pub args: Vec<LocalId>,

    // Constants (mutually exclusive and optional)
    pub func_id: FuncId,
    pub value: Value,
}

pub fn mir_from_typed_hir<'a>(typed_hir: &'a TypedHir<'a>) -> Result<Mir<'a>, Vec<Error<'a>>> {
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
    /// Total number of blocks, including partial ones.
    num_blocks: usize,
    current_block: (BlockId, Vec<Stmt>),
    queued_blocks: HashMap<BlockId, Vec<Stmt>>,
    finished_blocks: Vec<(BlockId, Block)>,
}

impl<'a> FuncBuilder<'a> {
    pub fn new(name: &'a str, num_params: usize) -> Self {
        Self {
            name,
            def_to_local: HashMap::new(),
            num_params,
            num_locals: 0,
            num_blocks: 1, // Only the current block exists at the start.
            current_block: (BlockId(0), Vec::new()),
            queued_blocks: HashMap::new(),
            finished_blocks: Vec::new(),
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

    fn push_stmt(&mut self, op: Op, args: Vec<LocalId>, func_id: FuncId, value: Value) -> LocalId {
        let local_id = self.new_local();
        self.current_block.1.push(Stmt {
            local_id,
            op,
            args,
            func_id,
            value,
        });
        local_id
    }

    pub fn push_op(&mut self, op: Op, args: Vec<LocalId>) -> LocalId {
        self.push_stmt(op, args, FuncId::DEFAULT, Value::Nothing)
    }

    pub fn push_constant(&mut self, value: Value) -> LocalId {
        self.push_stmt(Op::Constant, Vec::new(), FuncId::DEFAULT, value)
    }

    pub fn push_call(&mut self, func_id: FuncId, args: Vec<LocalId>) -> LocalId {
        self.push_stmt(Op::Call, args, func_id, Value::Nothing)
    }

    fn create_block_id(&mut self) -> BlockId {
        let id = BlockId(self.num_blocks);
        self.num_blocks += 1;
        id
    }

    /// Creates a new block and returns its ID, but does not enter it.
    pub fn create_block(&mut self) -> BlockId {
        let id = self.create_block_id();
        self.queued_blocks.insert(id, Vec::new());
        id
    }

    /// Sets the block currently being processed to `next_block_id`.
    /// This is assumed to be an empty block.
    pub fn set_current_block(&mut self, id: BlockId) {
        let block = self.queued_blocks.remove(&id).unwrap();
        let (old_id, old_block) = std::mem::replace(&mut self.current_block, (id, block));
        self.queued_blocks.insert(old_id, old_block);
    }

    pub fn finish_block(&mut self, branch: Branch) {
        let new_id = self.create_block_id();
        let (id, stmts) = std::mem::replace(&mut self.current_block, (new_id, Vec::new()));
        self.finished_blocks.push((id, Block { stmts, branch }));
    }

    pub fn build(self) -> Func<'a> {
        debug_assert!(self.current_block.1.is_empty());
        debug_assert!(self.queued_blocks.is_empty());

        // Replace placeholder block IDs with new ones now that their final positions are known.
        let mut blocks = Vec::with_capacity(self.finished_blocks.len());
        let mut id_map = Vec::from_iter(std::iter::repeat_n(
            BlockId(usize::MAX),
            self.finished_blocks.len(),
        ));
        for (new_id, (old_id, _)) in self.finished_blocks.iter().enumerate() {
            id_map[old_id.get()] = BlockId(new_id);
        }
        for (old_id, mut block) in self.finished_blocks {
            match &mut block.branch {
                Branch::Jump(target) => *target = id_map[target.get()],
                Branch::Cond { cond: _, target } => *target = id_map[target.get()],
                Branch::Return => {}
            }
            blocks[id_map[old_id.get()].get()] = block;
        }
        Func {
            name: self.name,
            // Return value + number of params + number of locals
            stack_space: 1 + self.num_params + self.num_locals,
            blocks,
        }
    }
}

struct Constructor<'a> {
    def_to_func: HashMap<DefId, FuncId>,
    errors: Errors<'a>,
}

impl<'a> Constructor<'a> {
    pub fn map_thir(typed_hir: &'a TypedHir<'a>) -> Result<Mir<'a>, Vec<Error<'a>>> {
        let mut _self = Self {
            def_to_func: typed_hir
                .funcs
                .iter()
                .enumerate()
                .map(|(i, func)| (func.def_id, FuncId(i)))
                .collect(),
            errors: Errors::new(typed_hir.filename),
        };
        let funcs = typed_hir
            .funcs
            .iter()
            .map(|func| _self.map_func(func))
            .collect();
        let mut builder = FuncBuilder::new("%main", 0);
        _self.map_scope(&typed_hir.stmts, None, Branch::Return, &mut builder);
        let main_func = builder.build();

        if !_self.errors.is_empty() {
            return Err(_self.errors.into_inner());
        }
        Ok(Mir {
            funcs,
            stack_space: main_func.stack_space,
            blocks: main_func.blocks,
        })
    }

    fn map_func(&mut self, func: &'a thir::Func<'a>) -> Func<'a> {
        let mut builder = FuncBuilder::new(func.name.0, func.params.len());
        self.map_scope(&func.body, None, Branch::Return, &mut builder);
        builder.build()
    }

    /// `loop_block_ids` are the block IDs for statements to jump to on `(continue, break)`.
    fn map_scope(
        &mut self,
        stmts: &'a [thir::Stmt<'a>],
        loop_block_ids: Option<(BlockId, BlockId)>,
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

                    use thir::AssignOp::*;
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
                thir::Stmt::If(thir::If { choices, r#else }) => {
                    let end_block_id = builder.create_block();
                    for (i, thir::Choice { cond, then }) in choices.iter().enumerate() {
                        let cond_id = self.map_expr(&cond.0, builder);
                        let next_block_id = if i + 1 == choices.len() {
                            end_block_id
                        } else {
                            builder.create_block()
                        };
                        let not_cond_id = builder.push_op(Op::Not, vec![cond_id]);
                        // If the condition is false, skip its 'then' block.
                        builder.finish_block(Branch::Cond {
                            cond: not_cond_id,
                            target: next_block_id,
                        });
                        self.map_scope(then, loop_block_ids, Branch::Jump(end_block_id), builder);
                        builder.set_current_block(next_block_id);
                    }
                    if let Some(r#else) = r#else {
                        self.map_scope(r#else, loop_block_ids, Branch::Jump(end_block_id), builder);
                    }
                    builder.set_current_block(end_block_id);
                }
                thir::Stmt::For(thir::For { iter, body, .. }) => {
                    // blocks:
                    // - init -> next
                    // - next -> body/end
                    // - body -> next
                    let end_block_id = builder.create_block();
                    // Init block
                    let into_iter_id = self.map_expr(&iter.0, builder);
                    let iter_id = builder.push_op(Op::IntoIter, vec![into_iter_id]);
                    let next_block_id = builder.create_block();
                    builder.finish_block(Branch::Jump(next_block_id));
                    // Next block
                    builder.set_current_block(next_block_id);
                    let var_id = builder.push_constant(Value::Nothing);
                    let cond_id = builder.push_op(Op::Next, vec![iter_id, var_id]);
                    let not_cond_id = builder.push_op(Op::Not, vec![cond_id]);
                    builder.finish_block(Branch::Cond {
                        cond: not_cond_id,
                        target: end_block_id,
                    });
                    // Body block
                    self.map_scope(
                        body,
                        Some((next_block_id, end_block_id)),
                        Branch::Jump(next_block_id),
                        builder,
                    );
                    builder.set_current_block(end_block_id);
                }
                thir::Stmt::While(thir::While { cond, body }) => {
                    // blocks:
                    // - cond -> body/end
                    // - body -> cond
                    let end_block_id = builder.create_block();
                    let cond_block_id = builder.create_block();
                    builder.finish_block(Branch::Jump(cond_block_id));
                    // Cond block
                    builder.set_current_block(cond_block_id);
                    let cond_id = self.map_expr(&cond.0, builder);
                    let not_cond_id = builder.push_op(Op::Not, vec![cond_id]);
                    builder.finish_block(Branch::Cond {
                        cond: not_cond_id,
                        target: end_block_id,
                    });
                    // Body block
                    self.map_scope(
                        body,
                        Some((cond_block_id, end_block_id)),
                        Branch::Jump(cond_block_id),
                        builder,
                    );
                    builder.set_current_block(end_block_id);
                }
                thir::Stmt::Loop(thir::Loop { body }) => {
                    // blocks:
                    // - body -> body/end
                    let end_block_id = builder.create_block();
                    let body_block_id = builder.create_block();
                    builder.finish_block(Branch::Jump(body_block_id));
                    // Body block
                    self.map_scope(
                        body,
                        Some((body_block_id, end_block_id)),
                        Branch::Jump(body_block_id),
                        builder,
                    );
                    builder.set_current_block(end_block_id);
                }
                thir::Stmt::Break(span) => match loop_block_ids {
                    Some((_, break_block_id)) => builder.finish_block(Branch::Jump(break_block_id)),
                    None => {
                        self.errors
                            .add("'break' may only be used inside of a loop.", *span);
                    }
                },
                thir::Stmt::Continue(span) => match loop_block_ids {
                    Some((continue_block_id, _)) => {
                        builder.finish_block(Branch::Jump(continue_block_id))
                    }
                    None => {
                        self.errors
                            .add("'continue' may only be used inside of a loop.", *span);
                    }
                },
                thir::Stmt::Return((value, _)) => {
                    if let Some(value) = value {
                        let value_id = self.map_expr(&value.0, builder);
                        builder.push_op(Op::Assign, vec![LocalId::RETURN_VALUE, value_id]);
                    }
                    builder.finish_block(Branch::Return);
                }
            }
        }
        // TODO: drop declared variables that hold pointers
        builder.finish_block(end_branch);
    }

    fn map_expr(&mut self, expr: &'a thir::Expr<'a>, builder: &mut FuncBuilder<'a>) -> LocalId {
        match expr {
            thir::Expr::Binary(thir::Binary { op, left, right }) => {
                let left_id = self.map_expr(&left.0, builder);
                let right_id = self.map_expr(&right.0, builder);
                use thir::BinaryOp::*;
                let op = match op.0 {
                    Add => Op::Add,
                    Sub => Op::Sub,
                    Mul => Op::Mul,
                    Div => Op::Div,
                    Eq => Op::Eq,
                    NotEq => Op::NotEq,
                };
                builder.push_op(op, vec![left_id, right_id])
            }
            thir::Expr::Unary(thir::Unary { op, expr }) => {
                let expr_id = self.map_expr(&expr.0, builder);
                use thir::UnaryOp::*;
                let op = match op.0 {
                    Neg => Op::Neg,
                    Not => Op::Not,
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
            thir::Expr::Int(int) => builder.push_constant(Value::Int(int.parse().unwrap())),
            thir::Expr::Bool(bool) => builder.push_constant(Value::Bool(*bool)),
            thir::Expr::String(string) => {
                builder.push_constant(Value::String(string.chars().collect()))
            }
            thir::Expr::Range(thir::Range { start, end }) => {
                let start_id = self.map_expr(&start.0, builder);
                let end_id = self.map_expr(&end.0, builder);
                builder.push_op(Op::Range, vec![start_id, end_id])
            }
        }
    }
}

use indentify::*;

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Nothing => f.write_str("Nothing"),
            Value::Int(int) => write!(f, "{int}"),
            Value::Bool(bool) => write!(f, "{bool}"),
            Value::String(chars) => chars.iter().map(|c| write!(f, "{c}")).collect(),
            Value::Range { start, end } => write!(f, "{start}..{end}"),
        }
    }
}

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

impl std::fmt::Display for Block {
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

impl std::fmt::Display for Stmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            local_id,
            op,
            args,
            func_id,
            value,
        } = self;
        write!(f, "var _{} = {op:?}", local_id.get())?;
        match op {
            Op::Constant => write!(f, "<{}>", value)?,
            Op::Call => write!(f, "<{:?}>", func_id.get())?,
            _ => {}
        }
        write!(f, "(",)?;
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "_{}", arg.get())?;
        }
        write!(f, ")")?;
        Ok(())
    }
}

impl std::fmt::Display for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Branch::Jump(target) => write!(f, "Jump(block {})", target.get()),
            Branch::Cond { cond, target } => {
                write!(f, "Cond({}, block {})", cond.get(), target.get())
            }
            Branch::Return => write!(f, "Return"),
        }
    }
}
