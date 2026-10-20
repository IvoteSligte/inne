use std::collections::{BTreeMap, HashMap};

pub use crate::typed_hir::{DefId, DefInfo, Int};
use crate::{
    Error, TypedHir,
    hir::{Errors, PRINT_DEF_ID},
    typed_hir as thir,
    util::write_comma_separated,
};

// TODO: reachability analysis
//       - check that non-void functions always return
//       - warn for unused code
//       - check for tautological conditions

// TODO: operator string(x) for value->string
// TODO: string concatenation
// TODO: print(...) without trailing newline

// TODO: make string and list slicing cheap
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String(Vec<char>),
    Char(char),
    List(Vec<Self>),
    Map(BTreeMap<Self, Self>),
    Range { start: Int, end: Int },
}

impl Value {
    pub fn print(&self) {
        match self {
            Value::Nothing => print!("Nothing"),
            Value::Int(int) => print!("{}", int),
            Value::Bool(bool) => print!("{}", bool),
            Value::String(chars) => {
                for char in chars {
                    print!("{}", char);
                }
            }
            Value::Char(char) => print!("{char}"),
            Value::List(elements) => {
                print!("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    elem.print();
                }
                print!("]");
            }
            Value::Map(pairs) => {
                print!("{{");
                for (i, (key, value)) in pairs.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    key.print();
                    print!(": ");
                    value.print();
                }
                print!("}}");
            }
            Value::Range { .. } => unreachable!(),
        }
    }
}

pub struct Mir<'a> {
    pub funcs: Vec<Func<'a>>,
    /// Stack space (in number of values) used by the global scope.
    pub stack_space: usize,
    /// Basic blocks in the global scope.
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

impl BlockId {
    pub const FINISHED: Self = Self(usize::MAX);
}

impl LocalId {
    pub const RETURN_VALUE: Self = Self(0);
}

#[derive(Debug)]
pub struct Func<'a> {
    pub name: &'a str,
    pub stack_space: usize,
    /// The basic blocks of the function.
    /// This is always at least 1 because the function has a return block.
    pub blocks: Vec<Block>,
}

/// A basic block.
#[derive(Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub branch: Branch,
}

#[derive(Debug)]
pub enum Branch {
    Jump(BlockId),
    Cond {
        cond: LocalId,
        true_target: BlockId,
        false_target: BlockId,
    },
    Return,
}

#[derive(Debug, Clone)]
pub enum Op {
    Assign { target: LocalId, value: LocalId },
    Add(Binary),
    Sub(Binary),
    Mul(Binary),
    Div(Binary),
    Mod(Binary),
    Eq(Binary),
    Neq(Binary),
    Lt(Binary),
    Leq(Binary),
    Gt(Binary),
    Geq(Binary),
    And(Binary),
    Or(Binary),
    Neg(LocalId),
    Not(LocalId),
    Constant(Value),
    Call { func_id: FuncId, args: Vec<LocalId> },
    List(Vec<LocalId>),
    Map(Vec<Pair>),
    Index { expr: LocalId, index: LocalId },
    Print { args: Vec<LocalId> },
    Range { start: LocalId, end: LocalId },
    IntoIter(LocalId),
    Next { target: LocalId, iter: LocalId },
}

#[derive(Debug, Clone, Copy)]
pub struct Binary {
    pub left: LocalId,
    pub right: LocalId,
}

#[derive(Debug, Clone, Copy)]
pub struct Pair {
    pub key: LocalId,
    pub value: LocalId,
}

// TODO: just add the args to Op, making it a tagged union
#[derive(Debug)]
pub struct Stmt {
    pub local_id: LocalId,
    pub op: Op,
}

pub fn mir_from_typed_hir<'a>(typed_hir: &'a TypedHir<'a>) -> Result<Mir<'a>, Vec<Error>> {
    Constructor::map_thir(typed_hir)
}

struct Constructor {
    def_to_func: HashMap<DefId, FuncId>,
    errors: Errors,
}

impl Constructor {
    pub fn map_thir<'a>(typed_hir: &'a TypedHir<'a>) -> Result<Mir<'a>, Vec<Error>> {
        let mut _self = Self {
            def_to_func: typed_hir
                .funcs
                .iter()
                .enumerate()
                .map(|(i, func)| (func.def_id, FuncId(i)))
                .collect(),
            errors: Errors::new(),
        };
        let funcs = typed_hir
            .funcs
            .iter()
            .map(|func| _self.map_func(func))
            .collect();
        let mut builder = FuncBuilder::new("%main");
        _self.map_scope(&typed_hir.stmts, None, &mut builder);
        builder.finish_block(Branch::Return);
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

    fn map_func<'a>(&mut self, func: &'a thir::Func<'a>) -> Func<'a> {
        let mut builder = FuncBuilder::new(func.name.0);
        for param in &func.params {
            let local_id = builder.create_local();
            builder.declare(param.def_id, local_id);
        }
        self.map_scope(&func.body, None, &mut builder);
        builder.finish_block(Branch::Return);
        builder.build()
    }

    /// `loop_block_ids` are the block IDs for statements to jump to on `(continue, break)`.
    fn map_scope<'a>(
        &mut self,
        stmts: &'a [thir::Stmt<'a>],
        loop_block_ids: Option<(BlockId, BlockId)>,
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
                        let args = Binary {
                            left: target_id,
                            right: value_id,
                        };
                        let op = match op.0 {
                            Add => Op::Add,
                            Sub => Op::Sub,
                            Mul => Op::Mul,
                            Div => Op::Div,
                            Mod => Op::Mod,
                            Assign => unreachable!(),
                        }(args);
                        value_id = builder.push_op(op);
                    }
                    builder.push_op(Op::Assign {
                        target: target_id,
                        value: value_id,
                    });
                }
                thir::Stmt::If(thir::If { choices, r#else }) => {
                    let end_block_id = builder.create_block();
                    for (i, thir::Choice { cond, then }) in choices.iter().enumerate() {
                        let cond_id = self.map_expr(&cond.0, builder);
                        let next_block_id = if i + 1 == choices.len() && r#else.is_none() {
                            end_block_id
                        } else {
                            builder.create_block()
                        };
                        // If the condition is false, skip its 'then' block.
                        let body_block_id = builder.create_block();
                        builder.cond_jump_to(cond_id, body_block_id, next_block_id);
                        self.map_scope(then, loop_block_ids, builder);
                        builder.push_jump(end_block_id, next_block_id);
                    }
                    if let Some(r#else) = r#else {
                        self.map_scope(r#else, loop_block_ids, builder);
                        builder.jump_to(end_block_id);
                    }
                }
                thir::Stmt::For(thir::For {
                    iter,
                    body,
                    def_id,
                    var: _,
                }) => {
                    // blocks:
                    // - init -> next
                    // - next -> body/end
                    // - body -> next
                    let end_block_id = builder.create_block();
                    // Init block
                    let into_iter_id = self.map_expr(&iter.0, builder);
                    let iter_id = builder.push_op(Op::IntoIter(into_iter_id));
                    let next_block_id = builder.create_block();
                    builder.jump_to(next_block_id);
                    // Next block
                    let var_id = builder.push_constant(Value::Nothing);
                    builder.declare(*def_id, var_id);
                    let cond_id = builder.push_op(Op::Next {
                        target: var_id,
                        iter: iter_id,
                    });
                    let body_block_id = builder.create_block();
                    builder.cond_jump_to(cond_id, body_block_id, end_block_id);
                    // Body block
                    self.map_scope(body, Some((next_block_id, end_block_id)), builder);
                    builder.push_jump(next_block_id, end_block_id);
                }
                thir::Stmt::While(thir::While { cond, body }) => {
                    // blocks:
                    // - cond -> body/end
                    // - body -> cond
                    let end_block_id = builder.create_block();
                    let cond_block_id = builder.create_block();
                    builder.jump_to(cond_block_id);
                    // Cond block
                    let cond_id = self.map_expr(&cond.0, builder);
                    let body_block_id = builder.create_block();
                    builder.cond_jump_to(cond_id, body_block_id, end_block_id);
                    // Body block
                    self.map_scope(body, Some((cond_block_id, end_block_id)), builder);
                    builder.push_jump(cond_block_id, end_block_id);
                }
                thir::Stmt::Loop(thir::Loop { body }) => {
                    // blocks:
                    // - body -> body/end
                    let end_block_id = builder.create_block();
                    let body_block_id = builder.create_block();
                    builder.jump_to(body_block_id);
                    // Body block
                    self.map_scope(body, Some((body_block_id, end_block_id)), builder);
                    builder.push_jump(body_block_id, end_block_id);
                }
                thir::Stmt::Break(span) => match loop_block_ids {
                    Some((_, break_block_id)) => {
                        builder.finish_block(Branch::Jump(break_block_id));
                        // TODO: warn for unreachable code if there are stmts after this 'break'
                        break;
                    }
                    None => {
                        self.errors
                            .add("'break' may only be used inside of a loop.", *span);
                    }
                },
                thir::Stmt::Continue(span) => match loop_block_ids {
                    Some((continue_block_id, _)) => {
                        builder.finish_block(Branch::Jump(continue_block_id));
                        // TODO: warn for unreachable code if there are stmts after this 'continue'
                        break;
                    }
                    None => {
                        self.errors
                            .add("'continue' may only be used inside of a loop.", *span);
                    }
                },
                thir::Stmt::Return((value, _)) => {
                    if let Some(value) = value {
                        let value_id = self.map_expr(&value.0, builder);
                        builder.push_op(Op::Assign {
                            target: LocalId::RETURN_VALUE,
                            value: value_id,
                        });
                    }
                    builder.finish_block(Branch::Return);
                    // TODO: warn for unreachable code if there are stmts after this 'return'
                    break;
                }
            }
        }
        // TODO: drop declared variables that hold pointers
    }

    fn map_expr<'a>(&mut self, expr: &'a thir::Expr<'a>, builder: &mut FuncBuilder<'a>) -> LocalId {
        match expr {
            thir::Expr::Binary(thir::Binary { op, left, right }) => {
                let left_id = self.map_expr(&left.0, builder);
                let right_id = self.map_expr(&right.0, builder);
                use thir::BinaryOp::*;
                let args = Binary {
                    left: left_id,
                    right: right_id,
                };
                let op = match op.0 {
                    Add => Op::Add,
                    Sub => Op::Sub,
                    Mul => Op::Mul,
                    Div => Op::Div,
                    Mod => Op::Mod,
                    Eq => Op::Eq,
                    NotEq => Op::Neq,
                    Lt => Op::Lt,
                    Leq => Op::Leq,
                    Gt => Op::Gt,
                    Geq => Op::Geq,
                    And => Op::And,
                    Or => Op::Or,
                }(args);
                builder.push_op(op)
            }
            thir::Expr::Unary(thir::Unary { op, expr }) => {
                let expr_id = self.map_expr(&expr.0, builder);
                use thir::UnaryOp::*;
                let op = match op.0 {
                    Neg => Op::Neg,
                    Not => Op::Not,
                }(expr_id);
                builder.push_op(op)
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
                    builder.push_op(Op::Print { args: arg_ids })
                } else {
                    let func_id = *self.def_to_func.get(def_id).unwrap();
                    builder.push_op(Op::Call {
                        func_id,
                        args: arg_ids,
                    })
                }
            }
            thir::Expr::Var(def_id) => builder.get_local(*def_id),
            thir::Expr::Int(int) => builder.push_constant(Value::Int(int.parse().unwrap())),
            thir::Expr::Bool(bool) => builder.push_constant(Value::Bool(*bool)),
            thir::Expr::String(string) => {
                builder.push_constant(Value::String(string.chars().collect()))
            }
            thir::Expr::Range(thir::Range { start, end }) => {
                let start = self.map_expr(&start.0, builder);
                let end = self.map_expr(&end.0, builder);
                builder.push_op(Op::Range { start, end })
            }
            thir::Expr::List(thir::List { elements }) => {
                let elements = elements
                    .iter()
                    .map(|elem| self.map_expr(&elem.0, builder))
                    .collect();
                builder.push_op(Op::List(elements))
            }
            thir::Expr::Map(thir::Map { pairs }) => {
                let pairs = pairs
                    .iter()
                    .map(|pair| Pair {
                        key: self.map_expr(&pair.key.0, builder),
                        value: self.map_expr(&pair.value.0, builder),
                    })
                    .collect();
                builder.push_op(Op::Map(pairs))
            }
            thir::Expr::Index(thir::Index { expr, index }) => {
                let expr = self.map_expr(&expr.0, builder);
                let index = self.map_expr(&index.0, builder);
                builder.push_op(Op::Index { expr, index })
            }
        }
    }
}

struct FuncBuilder<'a> {
    /// Function name.
    name: &'a str,
    /// Maps [DefId]s for variable declarations to their [LocalId].
    def_to_local: HashMap<DefId, LocalId>,
    /// Counter for the number of locals in this function.
    num_locals: usize,
    /// Total number of blocks, including partial ones.
    num_blocks: usize,
    current_block: (BlockId, Vec<Stmt>),
    queued_blocks: HashMap<BlockId, Vec<Stmt>>,
    finished_blocks: Vec<(BlockId, Block)>,
}

impl<'a> FuncBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            def_to_local: HashMap::new(),
            num_locals: 1, // Only the return value exists at the start.
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
        debug_assert!(
            self.def_to_local.contains_key(&def_id),
            "def_to_local does not contain def_id {def_id}; def_to_local: {:?}",
            self.def_to_local
        );
        // FIXME: this also retrieves globals and maps them to LocalIds, which is incorrect.
        *self.def_to_local.get(&def_id).unwrap()
    }

    /// Creates a local for a statement without requiring a body.
    pub fn create_local(&mut self) -> LocalId {
        let id = LocalId(self.num_locals);
        self.num_locals += 1;
        id
    }

    pub fn push_op(&mut self, op: Op) -> LocalId {
        debug_assert_ne!(self.current_block.0, BlockId::FINISHED);
        let local_id = self.create_local();
        self.current_block.1.push(Stmt { local_id, op });
        local_id
    }

    pub fn push_constant(&mut self, value: Value) -> LocalId {
        self.push_op(Op::Constant(value))
    }

    /// Creates a new block and returns its ID, but does not enter it.
    pub fn create_block(&mut self) -> BlockId {
        let id = BlockId(self.num_blocks);
        debug_assert!(id != BlockId::FINISHED);
        self.num_blocks += 1;
        self.queued_blocks.insert(id, Vec::new());
        id
    }

    /// Pushes an unconditional jump to `block_id` and also moves to it.
    pub fn jump_to(&mut self, block_id: BlockId) {
        debug_assert_ne!(self.current_block.0, block_id);
        self.push_jump(block_id, block_id);
    }

    /// Moves the cursor to `move_to_block_id` after pushing a jump to `target_block_id`.
    /// Does not push the jump (but still moves the cursor) if the current block is finished.
    pub fn push_jump(&mut self, target_block_id: BlockId, move_to_block_id: BlockId) {
        debug_assert_ne!(self.current_block.0, target_block_id);
        debug_assert_ne!(self.current_block.0, move_to_block_id);
        let (id, stmts) = std::mem::replace(
            &mut self.current_block,
            self.queued_blocks.remove_entry(&move_to_block_id).unwrap(),
        );
        if id != BlockId::FINISHED {
            let block = Block {
                stmts,
                branch: Branch::Jump(target_block_id),
            };
            self.finished_blocks.push((id, block));
        }
    }

    /// Moves the cursor to `true_id` after pushing a conditional jump to either `true_id` or `false_id`.
    /// Does not push the jump (but still moves the cursor) if the current block is finished.
    pub fn cond_jump_to(&mut self, cond: LocalId, true_id: BlockId, false_id: BlockId) {
        // Negates the condition because that tends to keep the MIR stmt ordering the same as in HIR.
        // NOTE: The Not needs to be pushed before replacing the current block.
        let not_cond = if self.current_block.0 != BlockId::FINISHED {
            self.push_op(Op::Not(cond))
        } else {
            LocalId(usize::MAX) // Not actually used, but satisfies the compiler.
        };
        let (id, stmts) = std::mem::replace(
            &mut self.current_block,
            self.queued_blocks.remove_entry(&true_id).unwrap(),
        );
        if id != BlockId::FINISHED {
            let block = Block {
                stmts,
                branch: Branch::Cond {
                    cond: not_cond,
                    true_target: false_id, // false_id because the condition is negated
                    false_target: true_id,
                },
            };
            self.finished_blocks.push((id, block));
        }
    }

    /// Finishes the current block and then moves the builder into the FINISHED state.
    /// This means that further writes should not be performed without moving
    /// the cursor to a different block first.
    /// If there current block is already finished, then this does nothing.
    pub fn finish_block(&mut self, branch: Branch) {
        if self.current_block.0 != BlockId::FINISHED {
            let (id, stmts) =
                std::mem::replace(&mut self.current_block, (BlockId::FINISHED, Vec::new()));
            let block = Block { stmts, branch };
            self.finished_blocks.push((id, block));
        }
    }

    pub fn build(mut self) -> Func<'a> {
        debug_assert!(self.current_block.1.is_empty());
        debug_assert!(!self.queued_blocks.contains_key(&BlockId::FINISHED));
        debug_assert!(self.queued_blocks.is_empty());

        // Replace placeholder block IDs with new ones now that their final positions are known.
        {
            let mut id_map = Vec::from_iter(std::iter::repeat_n(
                BlockId(usize::MAX),
                self.finished_blocks.len(),
            ));
            for (new_id, (old_id, _)) in self.finished_blocks.iter().enumerate() {
                debug_assert_ne!(*old_id, BlockId::FINISHED);
                id_map[old_id.get()] = BlockId(new_id);
            }
            for (_, block) in &mut self.finished_blocks {
                match &mut block.branch {
                    Branch::Jump(target) => *target = id_map[target.get()],
                    Branch::Cond {
                        cond: _,
                        true_target,
                        false_target,
                    } => {
                        *true_target = id_map[true_target.get()];
                        *false_target = id_map[false_target.get()];
                    }
                    Branch::Return => {}
                }
            }
        }
        Func {
            name: self.name,
            stack_space: self.num_locals,
            blocks: self
                .finished_blocks
                .into_iter()
                .map(|(_, block)| block)
                .collect(),
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
            Value::String(chars) => {
                f.write_str("\"")?;
                chars
                    .iter()
                    .map(|c| write!(f, "{c}"))
                    .collect::<std::fmt::Result>()?;
                f.write_str("\"")
            }
            Value::Char(char) => write!(f, "{char:?}"),
            Value::List(elements) => {
                f.write_str("[")?;
                write_comma_separated(f, elements)?;
                f.write_str("]")
            }
            Value::Map(pairs) => {
                f.write_str("{")?;
                for (i, pair) in pairs.into_iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    pair.0.fmt(f)?;
                    f.write_str(": ")?;
                    pair.1.fmt(f)?;
                }
                f.write_str("}")
            }
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
        writeln!(
            f,
            "func {name}({} locals) -> _0 {{{INDENT}",
            stack_space - 1
        )?;
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

fn write_tuple(elements: &[LocalId], f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "(",)?;
    for (i, element) in elements.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "_{}", element.get())?;
    }
    write!(f, ")")
}

impl std::fmt::Display for Stmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { local_id, op } = self;
        write!(f, "var _{} = {op:?}", local_id.get())?;
        use Op::*;
        match op {
            Constant(value) => write!(f, "<{}>", value),
            Call { func_id, args } => {
                write!(f, "<{:?}>", func_id.get())?;
                write_tuple(args, f)
            }
            List(elements) => {
                f.write_str("List(")?;
                write_comma_separated(f, elements.iter().map(|id| id.get()))?;
                f.write_str(")")
            }
            Map(pairs) => {
                f.write_str("Map(")?;
                for (i, pair) in pairs.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    pair.key.0.fmt(f)?;
                    f.write_str(": ")?;
                    pair.value.0.fmt(f)?;
                }
                f.write_str(")")
            }
            Index { expr, index } => {
                write!(f, "Index({}, {})", expr.get(), index.get())
            }
            Print { args } => {
                write!(f, "Print")?;
                write_tuple(args, f)
            }
            Assign { target, value } => {
                write!(f, "Assign({}, {})", target.get(), value.get())
            }
            Add(binary) | Sub(binary) | Mul(binary) | Div(binary) | Mod(binary) | Eq(binary)
            | Neq(binary) | Lt(binary) | Leq(binary) | Gt(binary) | Geq(binary) | And(binary)
            | Or(binary) => {
                let name = match op {
                    Add(_) => "Add",
                    Sub(_) => "Sub",
                    Mul(_) => "Mul",
                    Div(_) => "Div",
                    Mod(_) => "Mod",
                    Eq(_) => "Eq",
                    Neq(_) => "Neq",
                    Lt(_) => "Lt",
                    Leq(_) => "Leq",
                    Gt(_) => "Gt",
                    Geq(_) => "Geq",
                    And(_) => "And",
                    Or(_) => "Or",
                    _ => unreachable!(),
                };
                write!(f, "{}({}, {})", name, binary.left.get(), binary.right.get())
            }
            Neg(value) => write!(f, "Neg({})", value.get()),
            Not(value) => write!(f, "Not({})", value.get()),
            Range { start, end } => write!(f, "Range({}, {})", start.get(), end.get()),
            IntoIter(iter) => write!(f, "IntoIter({})", iter.get()),
            Next { target, iter } => write!(f, "Next({}, {})", target.get(), iter.get()),
        }
    }
}

impl std::fmt::Display for Branch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Branch::Jump(target) => write!(f, "Jump(block {})", target.get()),
            Branch::Cond {
                cond,
                true_target,
                false_target,
            } => {
                write!(
                    f,
                    "Cond(_{}, block {}, block {})",
                    cond.get(),
                    true_target.get(),
                    false_target.get()
                )
            }
            Branch::Return => write!(f, "Return"),
        }
    }
}
