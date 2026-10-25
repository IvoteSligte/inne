use crate::{
    Mir, Value,
    mir::*,
    value::{self, Int, create_pointer},
};

use std::mem::discriminant;

pub struct Interpreter<'a> {
    types: &'a [TypeInfo<'a>],
    funcs: &'a [Func<'a>],
    stack: Vec<Value>,
    frame_start: usize,
    frame_end: usize,
}

impl<'a> Interpreter<'a> {
    pub fn run(mir: &'a Mir<'a>) {
        let mut _self = Self {
            types: &mir.types,
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
                match self.get_local(target) {
                    Value::LocalPointer(local_id) => self.set_local(*local_id, value),
                    Value::FieldPointer { ptr, field_index } => {
                        ptr.borrow_mut().fields[*field_index] = value;
                    }
                    Value::List { ptr, start, end } => {
                        let dst = &mut ptr.borrow_mut()[*start..*end];
                        match value {
                            Value::List { ptr, start, end } => {
                                dst.clone_from_slice(&ptr.borrow()[start..end])
                            }
                            // TOOD: also allow copying from Pointer::Slice and Value::String (if list element type is char)
                            _ => unreachable!(),
                        }
                    }
                    Value::String { ptr, start, end } => {
                        let dst = &mut ptr.borrow_mut()[*start..*end];
                        match value {
                            Value::String { ptr, start, end } => {
                                dst.copy_from_slice(&ptr.borrow()[start..end])
                            }
                            // TOOD: also allow copying from Pointer::StringSlice and List(char)
                            _ => unreachable!(),
                        }
                    }
                    Value::ListIndexPointer { ptr, index } => ptr.borrow_mut()[*index] = value,
                    Value::StringIndexPointer { ptr: string, index } => {
                        string.borrow_mut()[*index] = match value {
                            Value::Char(char) => char,
                            _ => unreachable!(),
                        };
                    }
                    Value::MapValuePointer { ptr, key } => {
                        ptr.borrow_mut().insert((**key).clone(), value);
                    }
                    _ => unreachable!(),
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
                    self.print(self.get_local(*arg_id), false);
                }
                println!();
                Value::Nothing
            }
            &Op::Range { start, end } => match (self.get_local(start), self.get_local(end)) {
                (&Value::Int(start), &Value::Int(end)) => Value::Range { start, end },
                _ => unreachable!(),
            },
            // TODO: this should (deep)copy the original list/map, because shooting yourself
            // in the foot by modifying the original collection is way too easy
            &Op::IntoIter(iter) => match self.get_local(iter) {
                &Value::Range { start, end } => Value::Range { start, end },
                Value::List { ptr, start, end } => Value::List {
                    ptr: ptr.clone(),
                    start: *start,
                    end: *end,
                },
                Value::Map(ptr) => {
                    Value::MapIter(value::MapIter(value::Map::into_iter(ptr.borrow().clone())))
                }
                _ => unreachable!(),
            },
            &Op::Next { target, iter } => {
                let is_not_empty = match self.get_local_mut(iter) {
                    Value::Range { start, end } => {
                        let is_not_empty = *start < *end;
                        if is_not_empty {
                            let value = *start;
                            *start += 1;
                            self.set_local(target, Value::Int(value));
                        }
                        is_not_empty
                    }
                    Value::List { ptr, start, end } => {
                        let is_not_empty = start < end;
                        if is_not_empty {
                            let value = ptr.borrow()[*start].clone();
                            *start += 1;
                            self.set_local(target, value);
                        }
                        is_not_empty
                    }
                    Value::MapIter(iter) => {
                        let maybe_pair = iter.0.next();
                        let is_not_empty = maybe_pair.is_some();
                        if let Some((key, value)) = maybe_pair {
                            self.set_local(target, Value::new_tuple([key, value]));
                        }
                        is_not_empty
                    }
                    _ => unreachable!(),
                };
                Value::Bool(is_not_empty)
            }
            Op::List(local_ids) => Value::new_list(
                local_ids
                    .iter()
                    .map(|&local_id| self.get_local(local_id).clone()),
            ),
            Op::Map(pairs) => Value::new_map(pairs.iter().map(|&pair| {
                (
                    self.get_local(pair.key).clone(),
                    self.get_local(pair.value).clone(),
                )
            })),
            Op::Tuple(local_ids) => Value::new_tuple(
                local_ids
                    .iter()
                    .map(|&local_id| self.get_local(local_id).clone()),
            ),
            // TODO: require that the user explicitly specifies copy/ref instead of doing these expensive copies by default
            &Op::Index { expr, index } => match (self.get_local(expr), self.get_local(index)) {
                // TODO: out-of-bounds checks
                (Value::String { ptr, start, end: _ }, &Value::Int(i)) => {
                    Value::Char(ptr.borrow()[*start + i as usize])
                }
                (
                    Value::String { ptr, start, end: _ },
                    &Value::Range {
                        start: range_start,
                        end: range_end,
                    },
                ) => Value::String {
                    ptr: ptr.clone(),
                    start: *start + range_start as usize,
                    end: *start + range_end as usize,
                },
                (Value::List { ptr, start, end: _ }, &Value::Int(i)) => {
                    ptr.borrow()[*start + i as usize].clone()
                }
                (
                    Value::List { ptr, start, end: _ },
                    &Value::Range {
                        start: range_start,
                        end: range_end,
                    },
                ) => Value::List {
                    ptr: ptr.clone(),
                    start: *start + range_start as usize,
                    end: *start + range_end as usize,
                },
                // TODO: "key not in map" check
                (Value::Map(ptr), key) => ptr.borrow()[key].clone(),
                _ => unreachable!(),
            },
            Op::Struct { type_id, fields } => Value::new_struct(value::Struct {
                type_id: *type_id,
                fields: fields
                    .iter()
                    .map(|local_id| self.get_local(*local_id).clone())
                    .collect(),
            }),
            Op::Enum {
                type_id,
                variant_index,
                payload,
            } => Value::new_enum(value::Enum {
                type_id: *type_id,
                variant_index: *variant_index,
                payload: payload.map(|local_id| self.get_local(local_id).clone()),
            }),
            Op::Member { expr, field_index } => match self.get_local(*expr) {
                Value::Struct(ptr) => ptr.borrow().fields[*field_index].clone(),
                _ => unreachable!(),
            },
            Op::PointToLocal(local_id) => Value::LocalPointer(*local_id),
            Op::PointToField {
                r#struct,
                field_index,
            } => Value::FieldPointer {
                ptr: match self.get_local(*r#struct) {
                    Value::Struct(ptr) => ptr.clone(),
                    _ => unreachable!(),
                },
                field_index: *field_index,
            },
            Op::PointToIndex { collection, index } =>
            // TODO: check Int->usize casts and out-of-bounds
            {
                match (self.get_local(*collection), self.get_local(*index)) {
                    (Value::String { ptr, start, end: _ }, &Value::Int(index)) => {
                        Value::StringIndexPointer {
                            ptr: ptr.clone(),
                            index: *start + index as usize,
                        }
                    }
                    (
                        Value::String { ptr, start, end: _ },
                        &Value::Range {
                            start: range_start,
                            end: range_end,
                        },
                    ) => Value::String {
                        ptr: ptr.clone(),
                        start: *start + range_start as usize,
                        end: *start + range_end as usize,
                    },
                    (Value::List { ptr, start, end: _ }, &Value::Int(index)) => {
                        Value::ListIndexPointer {
                            ptr: ptr.clone(),
                            index: *start + index as usize,
                        }
                    }
                    (
                        Value::List { ptr, start, end: _ },
                        &Value::Range {
                            start: range_start,
                            end: range_end,
                        },
                    ) => Value::List {
                        ptr: ptr.clone(),
                        start: *start + range_start as usize,
                        end: *start + range_end as usize,
                    },
                    (Value::Map(ptr), key) => Value::MapValuePointer {
                        ptr: ptr.clone(),
                        key: Box::new(key.clone()),
                    },
                    _ => unreachable!(),
                }
            }
            Op::Copy(local_id) => self.deepcopy(self.get_local(*local_id)),
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

    pub fn print(&self, value: &Value, is_nested: bool) {
        match value {
            Value::Nothing => print!("Nothing"),
            Value::Int(int) => print!("{}", int),
            Value::Bool(bool) => print!("{}", bool),
            Value::String { ptr, start, end } => {
                let string = &ptr.borrow()[*start..*end];
                if is_nested {
                    print!("\"");
                    for char in string {
                        print!("{}", char.escape_default());
                    }
                    print!("\"");
                } else {
                    for char in string {
                        print!("{}", char);
                    }
                }
            }
            Value::Char(char) => print!("{char}"),
            Value::List { ptr, start, end } => {
                let list = &ptr.borrow()[*start..*end];
                print!("[");
                for (i, elem) in list.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    self.print(elem, true);
                }
                print!("]");
            }
            Value::Map(ptr) => {
                let map = ptr.borrow();
                print!("{{");
                for (i, (key, value)) in map.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    self.print(key, true);
                    print!(": ");
                    self.print(value, true);
                }
                print!("}}");
            }
            Value::Tuple(ptr) => {
                let tuple = ptr.borrow();
                print!("(");
                for (i, elem) in tuple.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    self.print(elem, true);
                }
                print!(")");
            }
            Value::Struct(ptr) => {
                let r#struct = ptr.borrow();
                let TypeInfo::Struct { name, field_names } = &self.types[r#struct.type_id.get()]
                else {
                    unreachable!();
                };
                debug_assert_eq!(field_names.len(), r#struct.fields.len());
                print!("{} {{", name);
                for (i, (name, value)) in std::iter::zip(field_names, &r#struct.fields).enumerate()
                {
                    if i > 0 {
                        print!(", ");
                    }
                    print!("{name}: ");
                    self.print(value, true);
                }
                print!("}}");
            }
            Value::Enum(ptr) => {
                let r#enum = ptr.borrow();
                let TypeInfo::Enum {
                    name,
                    variant_names,
                } = &self.types[r#enum.type_id.get()]
                else {
                    unreachable!();
                };
                debug_assert!(r#enum.variant_index < variant_names.len());
                print!("{}::{}", name, variant_names[r#enum.variant_index]);
                if let Some(payload) = r#enum.payload.as_ref() {
                    print!("(");
                    self.print(&payload, true);
                    print!(")");
                }
            }
            // Ranges currently cannot be used as values, only as for-loop iterator and in slicing operations.
            Value::Range { .. } => unreachable!(),
            // Pointers are currently not accessible to the user, so they are never printed.
            Value::LocalPointer(_)
            | Value::FieldPointer { .. }
            | Value::ListIndexPointer { .. }
            | Value::StringIndexPointer { .. }
            | Value::MapValuePointer { .. } => unimplemented!(),
            // MapIter is not exposed to the user and therefore never printed.
            Value::MapIter(_) => unimplemented!(),
        }
    }

    fn deepcopy(&self, value: &Value) -> Value {
        match value {
            Value::Nothing => Value::Nothing,
            Value::Int(int) => Value::Int(*int),
            Value::Bool(bool) => Value::Bool(*bool),
            Value::String { ptr, start, end } => {
                Value::new_string(ptr.borrow()[*start..*end].iter().copied())
            }
            Value::Char(char) => Value::Char(*char),
            Value::List { ptr, start, end } => Value::new_list(
                ptr.borrow()[*start..*end]
                    .iter()
                    .map(|element| self.deepcopy(element)),
            ),
            Value::Map(map) => Value::new_map(
                (*map.borrow())
                    .iter()
                    .map(|(key, value)| (self.deepcopy(key), self.deepcopy(value))),
            ),
            Value::Tuple(tuple) => {
                Value::new_tuple((*tuple).borrow().iter().map(|elem| self.deepcopy(elem)))
            }
            Value::Range { start, end } => Value::Range {
                start: *start,
                end: *end,
            },
            Value::Struct(ptr) => {
                let r#struct = ptr.borrow();
                Value::Struct(create_pointer(value::Struct {
                    type_id: r#struct.type_id,
                    fields: r#struct
                        .fields
                        .iter()
                        .map(|field| self.deepcopy(field))
                        .collect(),
                }))
            }
            Value::Enum(ptr) => {
                let r#enum = ptr.borrow();
                Value::Enum(create_pointer(value::Enum {
                    type_id: r#enum.type_id,
                    variant_index: r#enum.variant_index,
                    payload: r#enum
                        .payload
                        .as_ref()
                        .map(|payload| self.deepcopy(payload)),
                }))
            }
            Value::LocalPointer(local_id) => self.deepcopy(self.get_local(*local_id)),
            Value::FieldPointer { ptr, field_index } => {
                self.deepcopy(&ptr.borrow().fields[*field_index])
            }
            Value::ListIndexPointer { ptr, index } => self.deepcopy(&ptr.borrow()[*index]),
            Value::StringIndexPointer { ptr, index } => Value::Char(ptr.borrow()[*index]),
            Value::MapValuePointer { ptr, key } => self.deepcopy(&ptr.borrow()[key]),
            Value::MapIter(_) => unimplemented!("Map iterators should not be copied"),
        }
    }
}
