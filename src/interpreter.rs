use crate::{
    Mir, Value,
    mir::*,
    value::{self, Int, ValueOrPtr, create_pointer},
};

const NOTHING: ValueOrPtr = ValueOrPtr::NOTHING;

use std::{
    io::{Stdout, Write as _, stdout},
    mem::discriminant,
};

pub struct Interpreter<'a> {
    types: &'a [TypeInfo<'a>],
    funcs: &'a [Func<'a>],
    stack: Vec<ValueOrPtr>,
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
        _self.stack.extend((0..mir.stack_space).map(|_| NOTHING));
        _self.run_blocks(&mir.blocks);
    }

    /// Assumes that the stack frame is set to the given ID's frame.    
    fn get_local_or_ptr(&self, id: LocalId) -> &ValueOrPtr {
        debug_assert_ne!(id, LocalId::RETURN_VALUE);
        &self.stack[self.frame_start + id.get()]
    }

    /// Assumes that the stack frame is set to the given ID's frame.    
    fn get_local(&self, id: LocalId) -> &Value {
        match self.get_local_or_ptr(id) {
            ValueOrPtr::Value(value) => value,
            _ => unreachable!(),
        }
    }

    /// Assumes that the stack frame is set to the given ID's frame.
    /// Assumes that if `id` is [LocalId::RETURN_VALUE], that it is only written to, not read.
    fn get_local_or_ptr_mut(&mut self, id: LocalId) -> &mut ValueOrPtr {
        debug_assert_ne!(id, LocalId::RETURN_VALUE);
        &mut self.stack[self.frame_start + id.get()]
    }

    /// Assumes that the stack frame is set to the given ID's frame.
    fn set_local(&mut self, id: LocalId, value: impl Into<ValueOrPtr>) {
        self.stack[self.frame_start + id.get()] = value.into();
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
        let value_or_ptr: ValueOrPtr = match &stmt.op {
            &Op::Assign { target, value } => {
                let value = self.get_local(value).clone();
                // TODO: out-of-bounds checks
                match self.get_local_or_ptr(target) {
                    &ValueOrPtr::Ptr(abs_id) => self.stack[abs_id] = value.into(),
                    ValueOrPtr::FieldPtr { ptr, field_index } => {
                        ptr.borrow_mut().fields[*field_index] = value;
                    }
                    ValueOrPtr::ListIndexPtr { ptr, index } => ptr.borrow_mut()[*index] = value,
                    ValueOrPtr::MapValuePtr { ptr, key } => {
                        ptr.borrow_mut().insert(key.clone(), value);
                    }
                    _ => unreachable!(),
                };
                NOTHING
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
                Value::Bool(left == right).into()
            }
            &Op::Neq(Binary { left, right }) => {
                let left = self.get_local(left);
                let right = self.get_local(right);
                debug_assert_eq!(discriminant(left), discriminant(right));
                Value::Bool(left != right).into()
            }
            &Op::And(Binary { left, right }) => {
                match (self.get_local(left), self.get_local(right)) {
                    (&Value::Bool(left), &Value::Bool(right)) => Value::Bool(left && right).into(),
                    _ => unreachable!(),
                }
            }
            &Op::Or(Binary { left, right }) => {
                match (self.get_local(left), self.get_local(right)) {
                    (&Value::Bool(left), &Value::Bool(right)) => Value::Bool(left || right).into(),
                    _ => unreachable!(),
                }
            }
            &Op::Neg(value) => match self.get_local(value) {
                Value::Int(int) => Value::Int(-int).into(),
                _ => unreachable!(),
            },
            &Op::Not(value) => match self.get_local(value) {
                Value::Bool(bool) => Value::Bool(!bool).into(),
                _ => unreachable!(),
            },
            Op::Constant(value) => value.clone().into(),
            Op::Call { func_id, args } => {
                // Push stack frame.
                let func = &self.funcs[func_id.get()];
                let prev_frame_start = self.frame_start;
                let prev_frame_end = self.frame_end;
                self.frame_start = self.frame_end;
                self.frame_end = self.frame_start + func.stack_space;
                if self.stack.len() < self.frame_end {
                    self.stack
                        .extend((self.stack.len()..self.frame_end).map(|_| NOTHING));
                }
                // Default-initialize return value
                self.stack[self.frame_start] = NOTHING;
                // Move arguments to function parameter slots
                for (arg, i) in args.iter().zip(1..) {
                    self.stack[self.frame_start + i] =
                        std::mem::replace(&mut self.stack[prev_frame_start + arg.get()], NOTHING);
                }
                // Run function.
                self.run_func(func);
                // Move return value.
                self.stack[prev_frame_start + stmt.local_id.get()] =
                    std::mem::replace(&mut self.stack[self.frame_start], NOTHING);
                // Pop stack frame.
                self.frame_start = prev_frame_start;
                self.frame_end = prev_frame_end;
                self.stack[self.frame_end..]
                    .iter_mut()
                    .for_each(|item| *item = NOTHING);
                return; // The value is already on the stack.
            }
            Op::Print { args } => {
                struct StdoutWriter(Stdout);

                impl std::fmt::Write for StdoutWriter {
                    fn write_str(&mut self, s: &str) -> std::fmt::Result {
                        self.0.write_all(s.as_bytes()).map_err(|_| std::fmt::Error)
                    }
                }

                let mut writer = StdoutWriter(stdout());
                for (i, arg_id) in args.iter().enumerate() {
                    if i > 0 {
                        std::fmt::Write::write_str(&mut writer, " ").unwrap();
                    }
                    self.write_value_or_ptr(&mut writer, self.get_local_or_ptr(*arg_id))
                        .unwrap();
                }
                std::fmt::Write::write_str(&mut writer, "\n").unwrap();
                NOTHING
            }
            Op::Format { args } => {
                let mut out = String::new();
                for local_id in args {
                    let value_or_ptr = self.get_local_or_ptr(*local_id);
                    self.write_value_or_ptr(&mut out, value_or_ptr).unwrap();
                }
                Value::new_string(out.chars()).into()
            }
            &Op::Range { start, end } => match (self.get_local(start), self.get_local(end)) {
                (&Value::Int(start), &Value::Int(end)) => Value::Range { start, end }.into(),
                _ => unreachable!(),
            },
            // TODO: this should (deep)copy the original list/map, because shooting yourself
            // in the foot by modifying the original collection is way too easy
            &Op::IntoIter(iter) => match self.get_local(iter) {
                &Value::Range { start, end } => Value::Range { start, end }.into(),
                Value::List { ptr, is_string: _ } => ValueOrPtr::ListIter {
                    ptr: ptr.clone(),
                    index: 0,
                },
                Value::Map(ptr) => {
                    ValueOrPtr::MapIter(value::MapIter(value::Map::into_keys(ptr.borrow().clone())))
                }
                _ => unreachable!(),
            },
            &Op::Next { target, iter } => {
                let is_not_empty = match self.get_local_or_ptr_mut(iter) {
                    ValueOrPtr::Value(Value::Range { start, end }) => {
                        let is_not_empty = *start < *end;
                        if is_not_empty {
                            let value = *start;
                            *start += 1;
                            self.set_local(target, Value::Int(value));
                        }
                        is_not_empty
                    }
                    ValueOrPtr::ListIter { ptr, index } => {
                        let is_not_empty = *index < ptr.borrow().len();
                        if is_not_empty {
                            let value = ptr.borrow()[*index].clone();
                            *index += 1;
                            self.set_local(target, value);
                        }
                        is_not_empty
                    }
                    ValueOrPtr::MapIter(iter) => {
                        let maybe_key = iter.0.next();
                        let is_not_empty = maybe_key.is_some();
                        if let Some(key) = maybe_key {
                            self.set_local(target, key);
                        }
                        is_not_empty
                    }
                    _ => unreachable!(),
                };
                Value::Bool(is_not_empty).into()
            }
            Op::List(local_ids) => Value::new_list(
                local_ids
                    .iter()
                    .map(|&local_id| self.get_local(local_id).clone()),
                false,
            )
            .into(),
            Op::Map(pairs) => Value::new_map(pairs.iter().map(|&pair| {
                (
                    self.get_local(pair.key).clone(),
                    self.get_local(pair.value).clone(),
                )
            }))
            .into(),
            &Op::Index { expr, index } => match (self.get_local(expr), self.get_local(index)) {
                // TODO: out-of-bounds checks
                (Value::List { ptr, .. }, &Value::Int(i)) => ValueOrPtr::ListIndexPtr {
                    ptr: ptr.clone(),
                    index: i as _,
                },
                // TODO: "key not in map" check
                (Value::Map(ptr), key) => ValueOrPtr::MapValuePtr {
                    ptr: ptr.clone(),
                    key: self.deepcopy_value(key),
                },
                _ => unreachable!(),
            },
            Op::Struct { type_id, fields } => Value::new_struct(value::Struct {
                type_id: *type_id,
                fields: fields
                    .iter()
                    .map(|local_id| self.get_local(*local_id).clone())
                    .collect(),
            })
            .into(),
            Op::Enum {
                type_id,
                variant_index,
                payload,
            } => Value::new_enum(value::Enum {
                type_id: *type_id,
                variant_index: *variant_index,
                payload: payload.map(|local_id| self.get_local(local_id).clone()),
            })
            .into(),
            Op::Member { expr, field_index } => match self.get_local(*expr) {
                Value::Struct(ptr) => ValueOrPtr::FieldPtr {
                    ptr: ptr.clone(),
                    field_index: *field_index,
                },
                _ => unreachable!(),
            },
            Op::IsVariant {
                expr,
                variant_index,
            } => match self.get_local(*expr) {
                Value::Enum(ptr) => {
                    Value::Bool(ptr.borrow().variant_index == *variant_index).into()
                }
                _ => unreachable!(),
            },
            Op::GetPayload(expr) => match self.get_local(*expr) {
                Value::Enum(ptr) => ptr.borrow().payload.clone().unwrap().into(),
                _ => unreachable!(),
            },
            Op::PointToLocal(local_id) => ValueOrPtr::Ptr(self.frame_start + local_id.get()),
            Op::PointToField {
                r#struct,
                field_index,
            } => ValueOrPtr::FieldPtr {
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
                    (Value::List { ptr, .. }, &Value::Int(index)) => ValueOrPtr::ListIndexPtr {
                        ptr: ptr.clone(),
                        index: index as usize,
                    },
                    (Value::Map(ptr), key) => ValueOrPtr::MapValuePtr {
                        ptr: ptr.clone(),
                        key: self.deepcopy_value(key),
                    },
                    _ => unreachable!(),
                }
            }
            Op::Copy(local_id) => self
                .deepcopy_value_or_ptr(self.get_local_or_ptr(*local_id))
                .into(),
            Op::Deref(local_id) => self
                .apply_to_value(self.get_local_or_ptr(*local_id), Value::clone)
                .into(),
        };
        self.set_local(stmt.local_id, value_or_ptr);
    }

    fn int_op(&self, binary: Binary, f: impl FnOnce(Int, Int) -> Int) -> ValueOrPtr {
        match (self.get_local(binary.left), self.get_local(binary.right)) {
            (Value::Int(left), Value::Int(right)) => Value::Int(f(*left, *right)).into(),
            (Value::Nothing, _) => unreachable!("Left side is 'Nothing'"),
            (_, Value::Nothing) => unreachable!("Right side is 'Nothing'"),
            _ => unreachable!(),
        }
    }

    fn cmp_op(&self, binary: Binary, f: impl FnOnce(Int, Int) -> bool) -> ValueOrPtr {
        match (self.get_local(binary.left), self.get_local(binary.right)) {
            (Value::Int(left), Value::Int(right)) => Value::Bool(f(*left, *right)).into(),
            (Value::Nothing, _) => unreachable!("Left side is 'Nothing'"),
            (_, Value::Nothing) => unreachable!("Right side is 'Nothing'"),
            _ => unreachable!(),
        }
    }

    #[must_use]
    pub fn write_value_or_ptr(
        &self,
        f: &mut impl std::fmt::Write,
        value_or_ptr: &ValueOrPtr,
    ) -> std::fmt::Result {
        self.apply_to_value(value_or_ptr, |value| self.write_value(f, value, false))
    }

    #[must_use]
    pub fn write_value(
        &self,
        f: &mut impl std::fmt::Write,
        value: &Value,
        is_nested: bool,
    ) -> std::fmt::Result {
        match value {
            Value::Nothing => write!(f, "Nothing"),
            Value::Int(int) => write!(f, "{}", int),
            Value::Bool(bool) => write!(f, "{}", bool),
            Value::Char(char) => write!(f, "{char}"),
            Value::List { ptr, is_string } => {
                self.write_slice(f, &ptr.borrow(), *is_string, is_nested)
            }
            Value::Map(ptr) => {
                let map = ptr.borrow();
                write!(f, "{{")?;
                for (i, (key, value)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    self.write_value(f, key, true)?;
                    write!(f, ": ")?;
                    self.write_value(f, value, true)?;
                }
                write!(f, "}}")
            }
            Value::Struct(ptr) => {
                let r#struct = ptr.borrow();
                let TypeInfo::Struct { name, field_names } = &self.types[r#struct.type_id.get()]
                else {
                    unreachable!();
                };
                debug_assert_eq!(field_names.len(), r#struct.fields.len());
                write!(f, "{}:{{", name)?;
                if !field_names.is_empty() {
                    write!(f, " ")?;
                    for (i, (name, value)) in
                        std::iter::zip(field_names, &r#struct.fields).enumerate()
                    {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{name}: ")?;
                        self.write_value(f, value, true)?;
                    }
                    write!(f, " ")?;
                }
                write!(f, "}}")
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
                write!(f, "{}:{}", name, variant_names[r#enum.variant_index])?;
                if let Some(payload) = r#enum.payload.as_ref() {
                    write!(f, "(")?;
                    self.write_value(f, &payload, true)?;
                    write!(f, ")")?;
                }
                Ok(())
            }
            // Ranges currently cannot be used as values, only as for-loop iterator and in slicing operations.
            Value::Range { .. } => unreachable!(),
        }
    }

    #[must_use]
    fn write_slice(
        &self,
        f: &mut impl std::fmt::Write,
        slice: &[Value],
        is_string: bool,
        is_nested: bool,
    ) -> std::fmt::Result {
        if is_string {
            if is_nested {
                write!(f, "\"")?;
            }
            for value in slice {
                let Value::Char(char) = value else {
                    unreachable!();
                };
                write!(f, "{char}")?;
            }
            if is_nested {
                write!(f, "\"")?;
            }
            Ok(())
        } else {
            write!(f, "[")?;
            for (i, elem) in slice.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                self.write_value(f, elem, true)?;
            }
            write!(f, "]")
        }
    }

    fn apply_to_value<T>(&self, value_or_ptr: &ValueOrPtr, func: impl FnOnce(&Value) -> T) -> T {
        match value_or_ptr {
            ValueOrPtr::Value(value) => func(value),
            ValueOrPtr::Ptr(abs_id) => match &self.stack[*abs_id] {
                ValueOrPtr::Value(value) => func(value),
                _ => unreachable!(),
            },
            ValueOrPtr::FieldPtr { ptr, field_index } => func(&ptr.borrow().fields[*field_index]),
            ValueOrPtr::ListIndexPtr { ptr, index } => func(&ptr.borrow()[*index]),
            ValueOrPtr::MapValuePtr { ptr, key } => func(&ptr.borrow()[key]),
            ValueOrPtr::ListIter { .. } | ValueOrPtr::MapIter(_) => {
                unimplemented!("Iterators should not be exposed to the end user")
            }
        }
    }

    fn deepcopy_value_or_ptr(&self, value_or_ptr: &ValueOrPtr) -> Value {
        self.apply_to_value(value_or_ptr, |value| self.deepcopy_value(value))
    }

    fn deepcopy_value(&self, value: &Value) -> Value {
        match value {
            Value::Nothing => Value::Nothing,
            Value::Int(int) => Value::Int(*int),
            Value::Bool(bool) => Value::Bool(*bool),
            Value::Char(char) => Value::Char(*char),
            Value::List { ptr, is_string } => self.deepcopy_slice(&ptr.borrow(), *is_string),
            Value::Map(map) => Value::new_map(
                (*map.borrow())
                    .iter()
                    .map(|(key, value)| (self.deepcopy_value(key), self.deepcopy_value(value))),
            ),
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
                        .map(|field| self.deepcopy_value(field))
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
                        .map(|payload| self.deepcopy_value(payload)),
                }))
            }
        }
    }

    fn deepcopy_slice(&self, slice: &[Value], is_string: bool) -> Value {
        Value::new_list(
            slice.iter().map(|element| self.deepcopy_value(element)),
            is_string,
        )
    }
}
