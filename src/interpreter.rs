use crate::{
    Mir, Value,
    mir::*,
    value::{Int, ListPointer, Pointer, Struct, create_pointer},
};

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
    /// Assumes that if `id` is [LocalId::RETURN_VALUE], that it is only written to, not read.
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
            &Op::Assign { target, value } => {
                let value = self.get_local(value).clone();
                // TODO: out-of-bounds checks
                let Value::Pointer(pointer) = self.get_local(target) else {
                    unreachable!();
                };
                match pointer {
                    Pointer::Local(local_id) => self.set_local(*local_id, value),
                    Pointer::Field {
                        r#struct,
                        field_index,
                    } => {
                        r#struct.borrow_mut().fields[*field_index] = value;
                    }
                    Pointer::Slice { list, start, end } => {
                        let dest = &mut list.borrow_mut()[*start..*end];
                        match value {
                            Value::List(ptr) => dest.clone_from_slice(ptr.borrow().as_slice()),
                            // TOOD: also allow copying from Pointer::Slice and Value::String (if list element type is char)
                            _ => unreachable!(),
                        }
                    }
                    Pointer::ListIndex { list, index } => list.borrow_mut()[*index] = value,
                    Pointer::StringSlice { string, start, end } => {
                        let dest = &mut string.borrow_mut()[*start..*end];
                        match value {
                            Value::String(ptr) => dest.copy_from_slice(&ptr.borrow()),
                            // TOOD: also allow copying from Pointer::StringSlice and List(char)
                            _ => unreachable!(),
                        }
                    }
                    Pointer::StringIndex { string, index } => {
                        string.borrow_mut()[*index] = match value {
                            Value::Char(char) => char,
                            _ => unreachable!(),
                        };
                    }
                    Pointer::MapValue { map, key } => {
                        map.borrow_mut().insert((**key).clone(), value);
                    }
                };
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
                Value::List(ptr) => Value::ListIter {
                    list: ptr.clone(),
                    index: 0,
                },
                _ => unreachable!(),
            },
            &Op::Next { target, iter } => match self.get_local_mut(iter) {
                Value::Range { start, end } => {
                    let is_not_empty = *start < *end;
                    if is_not_empty {
                        let value = *start;
                        *start += 1;
                        self.set_local(target, Value::Int(value));
                    }
                    Value::Bool(is_not_empty)
                }
                Value::ListIter { list: ptr, index } => {
                    let list = ptr.borrow();
                    let is_empty = list.is_empty();
                    if !is_empty {
                        let value = list[*index].clone();
                        drop(list); // Explicitly drop list to resolve lifetime issues
                        *index += 1;
                        self.set_local(target, value);
                    }
                    Value::Bool(is_empty)
                }
                _ => unreachable!(),
            },
            Op::List(local_ids) => Value::List(create_pointer(
                local_ids
                    .iter()
                    .map(|&local_id| self.get_local(local_id).clone())
                    .collect(),
            )),
            Op::Map(pairs) => Value::Map(create_pointer(
                pairs
                    .iter()
                    .map(|&pair| {
                        (
                            self.get_local(pair.key).clone(),
                            self.get_local(pair.value).clone(),
                        )
                    })
                    .collect(),
            )),
            // TODO: require that the user explicitly specifies copy/ref instead of doing these expensive copies by default
            &Op::Index { expr, index } => match (self.get_local(expr), self.get_local(index)) {
                // TODO: out-of-bounds checks
                (Value::String(ptr), &Value::Int(i)) => Value::Char(ptr.borrow()[i as usize]),
                (Value::String(ptr), &Value::Range { start, end }) => Value::String(
                    create_pointer(ptr.borrow()[start as usize..end as usize].to_vec()),
                ),
                (Value::List(ptr), &Value::Int(i)) => ptr.borrow()[i as usize].clone(),
                (Value::List(ptr), &Value::Range { start, end }) => Value::List(create_pointer(
                    ptr.borrow()[start as usize..end as usize].to_vec(),
                )),
                // TODO: "key not in map" check
                (Value::Map(ptr), key) => ptr.borrow()[key].clone(),
                _ => unreachable!(),
            },
            Op::Struct { type_id, fields } => Value::Struct(create_pointer(Struct {
                type_id: *type_id,
                fields: fields
                    .iter()
                    .map(|local_id| self.get_local(*local_id).clone())
                    .collect(),
            })),
            Op::Member { expr, field_index } => match self.get_local(*expr) {
                Value::Struct(ptr) => ptr.borrow().fields[*field_index].clone(),
                _ => unreachable!(),
            },
            Op::PointToLocal(local_id) => Value::Pointer(Pointer::Local(*local_id)),
            Op::PointToField {
                r#struct,
                field_index,
            } => Value::Pointer(Pointer::Field {
                r#struct: match self.get_local(*r#struct) {
                    Value::Struct(ptr) => ptr.clone(),
                    _ => unreachable!(),
                },
                field_index: *field_index,
            }),
            Op::PointToIndex { collection, index } => Value::Pointer(
                // TODO: check Int->usize casts and out-of-bounds
                match (self.get_local(*collection), self.get_local(*index)) {
                    (Value::String(ptr), &Value::Int(index)) => Pointer::StringIndex {
                        string: ptr.clone(),
                        index: index as _,
                    },
                    (Value::String(ptr), &Value::Range { start, end }) => Pointer::StringSlice {
                        string: ptr.clone(),
                        start: start as _,
                        end: end as _,
                    },
                    (Value::List(ptr), &Value::Int(index)) => Pointer::ListIndex {
                        list: ptr.clone(),
                        index: index as _,
                    },
                    (Value::List(ptr), &Value::Range { start, end }) => Pointer::Slice {
                        list: ptr.clone(),
                        start: start as _,
                        end: end as _,
                    },
                    (Value::Map(ptr), key) => Pointer::MapValue {
                        map: ptr.clone(),
                        key: Box::new(key.clone()),
                    },
                    _ => unreachable!(),
                },
            ),
            Op::Copy(local_id) => match self.get_local(*local_id) {
                Value::Pointer(ptr) => self.deepcopy_pointer(ptr),
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

    fn deepcopy_list(&self, list: &ListPointer) -> ListPointer {
        create_pointer(
            (*list.borrow())
                .iter()
                .map(|element| self.deepcopy(element))
                .collect(),
        )
    }

    fn deepcopy(&self, value: &Value) -> Value {
        match value {
            Value::Nothing => Value::Nothing,
            Value::Int(int) => Value::Int(*int),
            Value::Bool(bool) => Value::Bool(*bool),
            Value::String(string) => Value::String(create_pointer((*string.borrow()).clone())),
            Value::Char(char) => Value::Char(*char),
            Value::List(list) => Value::List(self.deepcopy_list(list)),
            Value::ListIter { list, index } => Value::ListIter {
                list: self.deepcopy_list(list),
                index: *index,
            },
            Value::Map(map) => Value::Map(create_pointer(
                (*map.borrow())
                    .iter()
                    .map(|(key, value)| (self.deepcopy(key), self.deepcopy(value)))
                    .collect(),
            )),
            Value::Range { start, end } => Value::Range {
                start: *start,
                end: *end,
            },
            Value::Struct(ptr) => {
                let r#struct = ptr.borrow();
                Value::Struct(create_pointer(Struct {
                    type_id: r#struct.type_id,
                    fields: r#struct
                        .fields
                        .iter()
                        .map(|field| self.deepcopy(field))
                        .collect(),
                }))
            }
            Value::Pointer(ptr) => self.deepcopy_pointer(ptr),
        }
    }

    fn deepcopy_pointer(&self, pointer: &Pointer) -> Value {
        match pointer {
            Pointer::Local(local_id) => self.deepcopy(self.get_local(*local_id)),
            Pointer::Field {
                r#struct,
                field_index,
            } => self.deepcopy(&r#struct.borrow().fields[*field_index]),
            Pointer::Slice { list, start, end } => Value::List(create_pointer(
                list.borrow()[*start..*end]
                    .iter()
                    .map(|element| self.deepcopy(element))
                    .collect(),
            )),
            Pointer::ListIndex { list, index } => self.deepcopy(&list.borrow()[*index]),
            Pointer::StringSlice { string, start, end } => {
                Value::String(create_pointer(string.borrow()[*start..*end].to_vec()))
            }
            Pointer::StringIndex { string, index } => Value::Char(string.borrow()[*index]),
            Pointer::MapValue { map, key } => self.deepcopy(&map.borrow()[key]),
        }
    }
}
