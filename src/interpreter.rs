use crate::{Mir, mir::*};

use std::mem::discriminant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String(Vec<char>),
}

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
            frame_end: 0,
        };
        // Reserve space on the stack for global variables (and "return value")
        _self.stack.extend(std::iter::repeat_n(Value::Nothing, mir.stack_space));
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

    fn run_blocks(&mut self, blocks: &[Block<'a>]) {
        let mut block_index = 0;
        while block_index < blocks.len() {
            let block = &blocks[block_index];
            block_index += 1;

            for stmt in &block.stmts {
                // TODO: overflow checks
                let value = match stmt.op {
                    Op::Assign => {
                        self.set_local(stmt.args[0], self.get_local(stmt.args[1]).clone());
                        Value::Nothing
                    }
                    Op::Add => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(left + right),
                        _ => unreachable!(),
                    },
                    Op::Sub => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(left - right),
                        _ => unreachable!(),
                    },
                    Op::Mul => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(left * right),
                        _ => unreachable!(),
                    },
                    Op::Div => match (self.get_local(stmt.args[0]), self.get_local(stmt.args[1])) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(left / right),
                        _ => unreachable!(),
                    },
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
                    Op::Neg => match self.get_local(stmt.args[0]) {
                        Value::Int(int) => Value::Int(-int),
                        _ => unreachable!(),
                    },
                    Op::Not => match self.get_local(stmt.args[0]) {
                        Value::Bool(bool) => Value::Bool(!bool),
                        _ => unreachable!(),
                    },
                    Op::Int => Value::Int(stmt.int),
                    Op::String => Value::String(stmt.string.chars().collect()),
                    Op::Call => {
                        // Push stack frame.
                        let func = &self.funcs[stmt.func_id.get()];
                        let prev_frame_start = self.frame_start;
                        let prev_frame_end = self.frame_end;
                        // This ensures that the return value is placed at stmt.local_id
                        self.frame_start = self.frame_start + stmt.local_id.get();
                        self.frame_end = self.frame_start + func.stack_space;
                        if self.stack.len() < self.frame_end {
                            self.stack.extend(std::iter::repeat_n(
                                Value::Nothing,
                                self.frame_end - self.stack.len(),
                            ));
                        }
                        // Run function.
                        self.run_func(func);
                        // Pop stack frame.
                        self.frame_start = prev_frame_start;
                        self.frame_end = prev_frame_end;
                        continue; // The value is already on the stack.
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
                            }
                        }
                        println!();
                        Value::Nothing
                    }
                };
                self.set_local(stmt.local_id, value);
            }
            match &block.branch {
                Branch::Jump { target } => {
                    block_index = target.get();
                    continue;
                }
                Branch::Cond { cond, target } => {
                    let cond_value = self.get_local(*cond);
                    match cond_value {
                        Value::Bool(true) => {
                            block_index = target.get();
                            continue;
                        }
                        Value::Bool(false) => {}
                        _ => unreachable!(),
                    }
                }
                Branch::Return(local_id) => {
                    let value = match local_id {
                        Some(local_id) => self.get_local(*local_id).clone(),
                        None => Value::Nothing,
                    };
                    self.set_local(LocalId::RETURN_VALUE, value);
                    return;
                }
            }
        }
    }
}
