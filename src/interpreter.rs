use crate::{Mir, Value, mir::*, value::Int};

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
    fn get_local_mut(&mut self, id: LocalId) -> &mut Value {
        debug_assert_ne!(id, LocalId::RETURN_VALUE);
        &mut self.stack[self.frame_start + id.get()]
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
        let value = match &stmt.op {
            &Op::Assign {
                target,
                value,
                index: None,
            } => {
                self.set_local(target, self.get_local(value).clone());
                Value::Nothing
            }
            &Op::Assign {
                target,
                value,
                index: Some(index),
            } => {
                let value = self.get_local(value).clone();
                let index = self.get_local(index).clone();
                // TODO: out-of-bounds checks
                match (self.get_local_mut(target), index, value) {
                    (Value::String(chars), Value::Int(i), Value::Char(c)) => chars[i as usize] = c,
                    (Value::String(chars), Value::Range { start, end }, Value::String(s)) => {
                        chars[start as usize..end as usize].copy_from_slice(&s);
                    }
                    (Value::List(list), Value::Int(i), value) => list[i as usize] = value,
                    (Value::List(list), Value::Range { start, end }, Value::List(l)) => {
                        list[start as usize..end as usize].clone_from_slice(&l);
                    }
                    (Value::Map(map), key, value) => {
                        map.insert(key, value);
                    }
                    _ => unreachable!(),
                }
                Value::Nothing
            }
            &Op::Add(binary) => self.int_op(binary, |l, r| l + r),
            &Op::Sub(binary) => self.int_op(binary, |l, r| l - r),
            &Op::Mul(binary) => self.int_op(binary, |l, r| l * r),
            &Op::Div(binary) => self.int_op(binary, |l, r| l / r),
            &Op::Mod(binary) => self.int_op(binary, |l, r| l % r),
            &Op::Lt(binary) => self.cmp_op(binary, |l, r| l < r),
            &Op::Leq(binary) => self.cmp_op(binary, |l, r| l <= r),
            &Op::Gt(binary) => self.cmp_op(binary, |l, r| l > r),
            &Op::Geq(binary) => self.cmp_op(binary, |l, r| l >= r),
            &Op::Eq(Binary { left, right }) => {
                let left = self.get_local(left);
                let right = self.get_local(right);
                debug_assert_eq!(discriminant(left), discriminant(right));
                Value::Bool(left == right)
            }
            &Op::Neq(Binary { left, right }) => {
                let left = self.get_local(left);
                let right = self.get_local(right);
                debug_assert_eq!(discriminant(left), discriminant(right));
                Value::Bool(left != right)
            }
            &Op::And(Binary { left, right }) => {
                match (self.get_local(left), self.get_local(right)) {
                    (&Value::Bool(left), &Value::Bool(right)) => Value::Bool(left && right),
                    _ => unreachable!(),
                }
            }
            &Op::Or(Binary { left, right }) => {
                match (self.get_local(left), self.get_local(right)) {
                    (&Value::Bool(left), &Value::Bool(right)) => Value::Bool(left || right),
                    _ => unreachable!(),
                }
            }
            &Op::Neg(value) => match self.get_local(value) {
                Value::Int(int) => Value::Int(-int),
                _ => unreachable!(),
            },
            &Op::Not(value) => match self.get_local(value) {
                Value::Bool(bool) => Value::Bool(!bool),
                _ => unreachable!(),
            },
            Op::Constant(value) => value.clone(),
            Op::Call { func_id, args } => {
                // Push stack frame.
                let func = &self.funcs[func_id.get()];
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
                for (arg, i) in args.iter().zip(1..) {
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
            Op::Print { args } => {
                for (i, arg_id) in args.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    self.get_local(*arg_id).print();
                }
                println!();
                Value::Nothing
            }
            &Op::Range { start, end } => match (self.get_local(start), self.get_local(end)) {
                (&Value::Int(start), &Value::Int(end)) => Value::Range { start, end },
                _ => unreachable!(),
            },
            &Op::IntoIter(iter) => match self.get_local(iter) {
                &Value::Range { start, end } => Value::Range { start, end },
                // TODO: more efficient list iterator (should probably still be a "copy" of the original list, though,
                //       because shooting yourself in the foot by modifying the original list is way too easy)
                Value::List(elements) => Value::List(elements.clone()),
                _ => unreachable!(),
            },
            &Op::Next { target, iter } => match self.get_local(iter) {
                &Value::Range { start, end } => {
                    if start < end {
                        let new_range = Value::Range {
                            start: start + 1,
                            end,
                        };
                        self.set_local(iter, new_range);
                        self.set_local(target, Value::Int(start));
                    }
                    Value::Bool(start < end)
                }
                Value::List(elements) => {
                    let is_empty = elements.is_empty();
                    if !is_empty {
                        let new_list = Value::List(elements[1..].to_vec());
                        self.set_local(target, elements[0].clone());
                        self.set_local(iter, new_list);
                    }
                    Value::Bool(is_empty)
                }
                _ => unreachable!(),
            },
            Op::List(local_ids) => Value::List(
                local_ids
                    .iter()
                    .map(|&local_id| self.get_local(local_id).clone())
                    .collect(),
            ),
            Op::Map(pairs) => Value::Map(
                pairs
                    .iter()
                    .map(|&pair| {
                        (
                            self.get_local(pair.key).clone(),
                            self.get_local(pair.value).clone(),
                        )
                    })
                    .collect(),
            ),
            &Op::Index { expr, index } => match (self.get_local(expr), self.get_local(index)) {
                // TODO: out-of-bounds checks
                (Value::String(string), &Value::Int(i)) => Value::Char(string[i as usize]),
                (Value::String(string), &Value::Range { start, end }) => {
                    Value::String(string[start as usize..end as usize].to_vec())
                }
                (Value::List(elements), &Value::Int(i)) => elements[i as usize].clone(),
                (Value::List(elements), &Value::Range { start, end }) => {
                    Value::List(elements[start as usize..end as usize].to_vec())
                }
                // TODO: "key not in map" check
                (Value::Map(map), key) => map[key].clone(),
                _ => unreachable!(),
            },
        };
        self.set_local(stmt.local_id, value);
    }

    fn int_op(&self, binary: Binary, f: impl FnOnce(Int, Int) -> Int) -> Value {
        match (self.get_local(binary.left), self.get_local(binary.right)) {
            (Value::Int(left), Value::Int(right)) => Value::Int(f(*left, *right)),
            (Value::Nothing, _) => unreachable!("Left side is 'Nothing'"),
            (_, Value::Nothing) => unreachable!("Right side is 'Nothing'"),
            _ => unreachable!(),
        }
    }

    fn cmp_op(&self, binary: Binary, f: impl FnOnce(Int, Int) -> bool) -> Value {
        match (self.get_local(binary.left), self.get_local(binary.right)) {
            (Value::Int(left), Value::Int(right)) => Value::Bool(f(*left, *right)),
            (Value::Nothing, _) => unreachable!("Left side is 'Nothing'"),
            (_, Value::Nothing) => unreachable!("Right side is 'Nothing'"),
            _ => unreachable!(),
        }
    }
}
