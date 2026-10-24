use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use crate::{hir::TypeId, mir::LocalId};

pub type Int = i64;

// TODO: make string and list slicing cheap
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String {
        ptr: StringPointer,
        start: usize,
        end: usize,
    },
    Char(char),
    List {
        ptr: ListPointer,
        start: usize,
        end: usize,
    },
    Map(MapPointer),
    Range {
        start: Int,
        end: Int,
    },
    Struct(StructPointer),
    /// Pointer to a local value. Only valid for the duration of the current function.
    LocalPointer(LocalId),
    FieldPointer {
        ptr: StructPointer,
        field_index: usize,
    },
    ListIndexPointer {
        ptr: ListPointer,
        index: usize,
    },
    StringIndexPointer {
        ptr: StringPointer,
        index: usize,
    },
    MapValuePointer {
        ptr: MapPointer,
        key: Box<Value>,
    },
}

impl Value {
    pub fn new_list(elements: Vec<Value>) -> Self {
        Self::List {
            start: 0,
            end: elements.len(),
            ptr: create_pointer(elements),
        }
    }

    pub fn new_string(chars: Vec<char>) -> Self {
        Self::String {
            start: 0,
            end: chars.len(),
            ptr: create_pointer(chars),
        }
    }
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

pub fn create_pointer<T>(value: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(value))
}
