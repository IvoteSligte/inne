use crate::typed_hir::*;

pub enum Value {
    Int(Int),
    Bool(bool),
    String(Vec<char>),
}

pub struct Interpreter {}

impl Interpreter {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run<'a>(&mut self, typed_hir: &'a TypedHir<'a>) {
        // Runs the top-level statements.
        self.run_func(main_func, &[]);
    }

    fn run_func<'a>(&mut self, func: &'a Func<'a>, args: &[Value]) -> Value {
        debug_assert_eq!(func.params.len(), args.len());
        todo!()
    }
}
