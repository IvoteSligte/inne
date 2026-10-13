use crate::{Mir, mir::*};

pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String(Vec<char>),
}

pub struct Interpreter {
    stack: Vec<Value>,
    frame_start: usize,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(1000),
            frame_start: 0,
        }
    }

    pub fn run<'a>(&mut self, mir: &'a Mir<'a>) {
        for block in &mir.blocks {
            todo!()
        }
    }

    /// Assumes that the stack frame is set to the given ID's frame.    
    fn get_local(&self, id: LocalId) -> Value {
        debug_assert_ne!(id, LocalId::RETURN_VALUE);
        self.stack[self.frame_start + id.get()]
    }

    /// Assumes that the stack frame is set to the given ID's frame.
    fn set_local(&self, id: LocalId, value: Value) {
        self.stack[self.frame_start + id.get()] = value;
    }

    fn run_func<'a>(&mut self, func: &'a Func<'a>) -> Value {
        for block in &func.blocks {
            for stmt in &block.stmts {
                match stmt.op {
                    Op::Assign =>
                    // TODO: this still needs to push () onto the stack (unless avoided in MIR)
                    {
                        self.set_local(stmt.args[0], self.get_local(stmt.args[1]))
                    }
                    Op::Add => todo!(),
                    Op::Sub => todo!(),
                    Op::Mul => todo!(),
                    Op::Div => todo!(),
                    Op::Eq => todo!(),
                    Op::NotEq => todo!(),
                    Op::Neg => todo!(),
                    Op::Not => todo!(),
                    Op::Int => todo!(),
                    Op::String => todo!(),
                    Op::Call => todo!(),
                }
            }
            match &block.branch {
                Branch::Jump { target } => {
                    let target_block = &func.blocks[target.get()];
                    todo!()
                }
                Branch::Cond { cond, target } => {
                    let cond_value = self.get_local(*cond);
                    match cond_value {
                        Value::Bool(true) => {
                            let target_block = &func.blocks[target.get()];
                            todo!()
                        }
                        Value::Bool(false) => {}
                        _ => unreachable!(),
                    }
                }
                Branch::Return(local_id) => {
                    return match local_id {
                        Some(local_id) => self.get_local(*local_id),
                        None => Value::Nothing,
                    };
                }
            }
        }
        unreachable!()
    }
}
