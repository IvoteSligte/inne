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
        ptr: Ptr<String>,
        start: usize,
        end: usize,
    },
    Char(char),
    List {
        ptr: Ptr<List>,
        start: usize,
        end: usize,
    },
    Map(Ptr<Map>),
    Range {
        start: Int,
        end: Int,
    },
    Struct(Ptr<Struct>),
    Enum(Ptr<Enum>),
    /// Pointer to a local value. Only valid for the duration of the current function.
    LocalPointer(LocalId),
    FieldPointer {
        ptr: Ptr<Struct>,
        field_index: usize,
    },
    ListIndexPointer {
        ptr: Ptr<List>,
        index: usize,
    },
    StringIndexPointer {
        ptr: Ptr<String>,
        index: usize,
    },
    MapValuePointer {
        ptr: Ptr<Map>,
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

    pub fn new_struct(r#struct: Struct) -> Self {
        Self::Struct(create_pointer(r#struct))
    }

    pub fn new_enum(r#enum: Enum) -> Self {
        Self::Enum(create_pointer(r#enum))
    }
}

type Ptr<T> = Rc<RefCell<T>>;

pub type String = Vec<char>;
pub type List = Vec<Value>;
pub type Map = BTreeMap<Value, Value>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Struct {
    pub type_id: TypeId,
    pub fields: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Enum {
    pub type_id: TypeId,
    pub variant_index: usize,
    pub payload: Option<Value>,
}

/// Wrapper around a [BTreeMap] iterator that implements traits that all [Value]s must have.
/// These traits should not be used, however.
#[derive(Debug)]
pub struct MapIter(pub std::collections::btree_map::IntoKeys<Value, Value>);

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

pub fn create_pointer<T>(value: T) -> Ptr<T> {
    Rc::new(RefCell::new(value))
}
