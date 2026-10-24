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
    /// Tuple does not use a pointer as it is currently impossible to take a pointer
    /// to any of its elements.
    Tuple(TuplePointer),
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
    MapIter(MapIter),
}

impl Value {
    pub fn new_list(elements: impl IntoIterator<Item = Value>) -> Self {
        let elements = elements.into_iter().collect::<List>();
        Self::List {
            start: 0,
            end: elements.len(),
            ptr: create_pointer(elements),
        }
    }

    pub fn new_string(chars: impl IntoIterator<Item = char>) -> Self {
        let chars = chars.into_iter().collect::<String>();
        Self::String {
            start: 0,
            end: chars.len(),
            ptr: create_pointer(chars),
        }
    }

    pub fn new_map(pairs: impl IntoIterator<Item = (Value, Value)>) -> Self {
        Self::Map(create_pointer(pairs.into_iter().collect()))
    }

    pub fn new_tuple(elements: impl IntoIterator<Item = Value>) -> Self {
        Self::Tuple(create_pointer(elements.into_iter().collect()))
    }
}

pub type String = Vec<char>;
pub type StringPointer = Rc<RefCell<String>>;

pub type List = Vec<Value>;
pub type ListPointer = Rc<RefCell<List>>;

pub type Map = BTreeMap<Value, Value>;
pub type MapPointer = Rc<RefCell<Map>>;

pub type Tuple = Vec<Value>;
pub type TuplePointer = Rc<RefCell<Tuple>>;

pub type StructPointer = Rc<RefCell<Struct>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Struct {
    pub type_id: TypeId,
    pub fields: Vec<Value>,
}

/// Wrapper around a [BTreeMap] iterator that implements traits that all [Value]s must have.
/// These traits should not be used, however.
#[derive(Debug)]
pub struct MapIter(pub std::collections::btree_map::IntoIter<Value, Value>);

impl Clone for MapIter {
    fn clone(&self) -> Self {
        unreachable!("MapIter cannot be cloned")
    }
}

impl PartialEq for MapIter {
    fn eq(&self, _other: &Self) -> bool {
        unreachable!("MapIter cannot be compared for equality")
    }
}

impl Eq for MapIter {}

impl PartialOrd for MapIter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MapIter {
    fn cmp(&self, _other: &Self) -> std::cmp::Ordering {
        unreachable!("MapIter cannot be compared for ordering")
    }
}

pub fn create_pointer<T>(value: T) -> Rc<RefCell<T>> {
    Rc::new(RefCell::new(value))
}
