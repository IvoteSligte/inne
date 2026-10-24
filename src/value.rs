use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use crate::{hir::TypeId, mir::LocalId};

pub type Int = i64;

// TODO: make string and list slicing cheap
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String(StringPointer),
    Char(char),
    List(ListPointer),
    ListIter { list: ListPointer, index: usize },
    Map(MapPointer),
    Range { start: Int, end: Int },
    Struct(StructPointer),
    Pointer(Pointer),
}

pub type String = Vec<char>;

pub type StringPointer = Rc<RefCell<String>>;

pub type List = Vec<Value>;

pub type ListPointer = Rc<RefCell<List>>;

pub type Map = BTreeMap<Value, Value>;

pub type MapPointer = Rc<RefCell<Map>>;

pub type StructPointer = Rc<RefCell<Struct>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Struct {
    pub type_id: TypeId,
    pub fields: Vec<Value>,
}

impl Value {
    pub fn print(&self) {
        match self {
            Value::Nothing => print!("Nothing"),
            Value::Int(int) => print!("{}", int),
            Value::Bool(bool) => print!("{}", bool),
            Value::String(ptr) => {
                for char in ptr.borrow().as_slice() {
                    print!("{}", char);
                }
            }
            Value::Char(char) => print!("{char}"),
            Value::List(ptr) => {
                let list = ptr.borrow();
                print!("[");
                for (i, elem) in list.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    elem.print();
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
                    key.print();
                    print!(": ");
                    value.print();
                }
                print!("}}");
            }
            Value::Range { .. } => unreachable!(),
            Value::Struct(ptr) => {
                let r#struct = ptr.borrow();
                // TODO: print type name and field names
                print!("struct {{");
                for (i, value) in r#struct.fields.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    value.print();
                }
                print!("}}");
            }
            // Pointer and ListIter are currently not items accessible to the user, so they should never be printed.
            Value::Pointer(_) => unimplemented!(),
            Value::ListIter { .. } => unimplemented!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pointer {
    /// Pointer to a local value. Only valid for the duration of the current function.
    Local(LocalId),
    Field {
        r#struct: StructPointer,
        field_index: usize,
    },
    Slice {
        list: ListPointer,
        start: usize,
        end: usize,
    },
    ListIndex {
        list: ListPointer,
        index: usize,
    },
    StringSlice {
        string: StringPointer,
        start: usize,
        end: usize,
    },
    StringIndex {
        string: StringPointer,
        index: usize,
    },
    MapValue {
        map: MapPointer,
        key: Box<Value>,
    },
}

pub fn create_pointer<T>(value: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(value))
}
