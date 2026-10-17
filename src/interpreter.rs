use crate::{Mir, mir::*};

use std::mem::discriminant;

pub struct Interpreter<'a> {
    funcs: &'a [Func<'a>],
    stack: Vec<Value>,
    frame_start: usize,
    frame_end: usize,
}

impl<'a> Interpreter<'a> {
    pub fn run(mir: &'a Mir<'a>) {
        let mut _self = Self {
            funcs: &mir.funcs,
            stack: Vec::with_capacity(1000),
            frame_start: 0,
            frame_end: mir.stack_space,
        };
        // Reserve space on the stack for global variables (and "return value")
        _self
            .stack
            .extend(std::iter::repeat_n(Value::Nothing, mir.stack_space));
        _self.run_blocks(&mir.blocks);
    }

    /// Assumes that the stack frame is set to the given ID's frame.    
    fn get_local(&self, id: LocalId) -> &Value {
        debug_assert_ne!(id, LocalId::RETURN_VALUE);
        &self.stack[self.frame_start + id.get()]
    }

    /// Assumes that the stack frame is set to the given ID's frame.
    fn set_local(&mut self, id: LocalId, value: Value) {
        self.stack[self.frame_start + id.get()] = value;
    }

    /// The caller is expected to allocate enough stack space for this function's stack frame.
    fn run_func(&mut self, func: &'a Func<'a>) {
        self.run_blocks(&func.blocks);
    }

    fn run_blocks(&mut self, blocks: &[Block]) {
        let mut block_index = 0;
        while block_index < blocks.len() {
            let block = &blocks[block_index];
            for stmt in &block.stmts {
                self.run_stmt(stmt);
            }
            block_index = match &block.branch {
                Branch::Jump(target) => target.get(),
                Branch::Cond {
                    cond,
                    true_target,
                    false_target,
                } => match self.get_local(*cond) {
                    Value::Bool(true) => true_target.get(),
                    Value::Bool(false) => false_target.get(),
                    Value::Nothing => unreachable!("'Nothing' was used as condition."),
                    _ => unreachable!("A non-boolean type was used as condition."),
                },
                Branch::Return => break,
            };
        }
    }

    fn run_stmt(&mut self, stmt: &Stmt) {
        // TODO: arithmetic overflow checks
        let value = match stmt.op {
            Op::Assign => {
                self.set_local(stmt.args[0], self.get_local(stmt.args[1]).clone());
                Value::Nothing
            }
            Op::Add => self.int_op(stmt, |l, r| l + r),
            Op::Sub => self.int_op(stmt, |l, r| l - r),
            Op::Mul => self.int_op(stmt, |l, r| l * r),
            Op::Div => self.int_op(stmt, |l, r| l / r),
            Op::Mod => self.int_op(stmt, |l, r| l % r),
            Op::Lt => self.cmp_op(stmt, |l, r| l < r),
            Op::Leq => self.cmp_op(stmt, |l, r| l <= r),
            Op::Gt => self.cmp_op(stmt, |l, r| l > r),
            Op::Geq => self.cmp_op(stmt, |l, r| l >= r),
            Op::Eq => {
                let left = self.get_local(stmt.args[0]);
                let right = self.get_local(stmt.args[1]);
                debug_assert_eq!(discriminant(left), discriminant(right));
                Value::Bool(left == right)
            }
            Op::NotEq => {
                let left = self.get_local(stmt.args[0]);
                let right = self.get_local(stmt.args[1]);
                debug_assert_eq!(discriminant(left), discriminant(right));
                Value::Bool(left != right)
            }
            Op::And => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                (&Value::Bool(left), &Value::Bool(right)) => Value::Bool(left && right),
                _ => unreachable!(),
            },
            Op::Or => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                (&Value::Bool(left), &Value::Bool(right)) => Value::Bool(left || right),
                _ => unreachable!(),
            },
            Op::Neg => match self.get_local(stmt.args[0]) {
                Value::Int(int) => Value::Int(-int),
                _ => unreachable!(),
            },
            Op::Not => match self.get_local(stmt.args[0]) {
                Value::Bool(bool) => Value::Bool(!bool),
                _ => unreachable!(),
            },
            Op::Constant => stmt.value.clone(),
            Op::Call => {
                // Push stack frame.
                let func = &self.funcs[stmt.func_id.get()];
                let prev_frame_start = self.frame_start;
                let prev_frame_end = self.frame_end;
                self.frame_start = self.frame_end;
                self.frame_end = self.frame_start + func.stack_space;
                if self.stack.len() < self.frame_end {
                    self.stack.extend(std::iter::repeat_n(
                        Value::Nothing,
                        self.frame_end - self.stack.len(),
                    ));
                }
                // Default-initialize return value
                self.stack[self.frame_start] = Value::Nothing;
                // Move arguments to function parameter slots
                for (arg, i) in stmt.args.iter().zip(1..) {
                    self.stack[self.frame_start + i] = std::mem::replace(
                        &mut self.stack[prev_frame_start + arg.get()],
                        Value::Nothing,
                    );
                }
                // Run function.
                self.run_func(func);
                // Move return value
                self.stack[prev_frame_start + stmt.local_id.get()] =
                    std::mem::replace(&mut self.stack[self.frame_start], Value::Nothing);
                // Pop stack frame.
                self.frame_start = prev_frame_start;
                self.frame_end = prev_frame_end;
                self.stack[self.frame_end..].fill(Value::Nothing);
                return; // The value is already on the stack.
            }
            Op::Print => {
                for (i, arg_id) in stmt.args.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    match self.get_local(*arg_id) {
                        Value::Nothing => print!("Nothing"),
                        Value::Int(int) => print!("{}", int),
                        Value::Bool(bool) => print!("{}", bool),
                        Value::String(chars) => {
                            for char in chars {
                                print!("{}", char);
                            }
                        }
                        Value::Range { .. } => unreachable!(),
                    }
                }
                println!();
                Value::Nothing
            }
            Op::Range => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                (&Value::Int(start), &Value::Int(end)) => Value::Range { start, end },
                _ => unreachable!(),
            },
            Op::IntoIter => match self.get_local(stmt.args[0]) {
                &Value::Range { start, end } => Value::Range { start, end },
                _ => unreachable!(),
            },
            Op::Next => match self.get_local(stmt.args[0]) {
                &Value::Range { start, end } => {
                    if start < end {
                        let new_range = Value::Range {
                            start: start + 1,
                            end,
                        };
                        self.set_local(stmt.args[0], new_range);
                        self.set_local(stmt.args[1], Value::Int(start));
                    }
                    Value::Bool(start < end)
                }
                _ => unreachable!(),
            },
        };
        self.set_local(stmt.local_id, value);
    }

    fn int_op(&self, stmt: &Stmt, f: impl FnOnce(Int, Int) -> Int) -> Value {
        match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
            (Value::Int(left), Value::Int(right)) => Value::Int(f(*left, *right)),
            (Value::Nothing, _) => unreachable!("Left side is 'Nothing'"),
            (_, Value::Nothing) => unreachable!("Right side is 'Nothing'"),
            _ => unreachable!(),
        }
    }

    fn cmp_op(&self, stmt: &Stmt, f: impl FnOnce(Int, Int) -> bool) -> Value {
        match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
            (Value::Int(left), Value::Int(right)) => Value::Bool(f(*left, *right)),
            (Value::Nothing, _) => unreachable!("Left side is 'Nothing'"),
            (_, Value::Nothing) => unreachable!("Right side is 'Nothing'"),
            _ => unreachable!(),
        }
    }
}
