use std::collections::BTreeMap;

pub type Int = i64;

// TODO: make string and list slicing cheap
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Nothing,
    Int(Int),
    Bool(bool),
    String(Vec<char>),
    Char(char),
    List(Vec<Self>),
    Map(BTreeMap<Self, Self>),
    Range { start: Int, end: Int },
}

impl Value {
    pub fn print(&self) {
        match self {
            Value::Nothing => print!("Nothing"),
            Value::Int(int) => print!("{}", int),
            Value::Bool(bool) => print!("{}", bool),
            Value::String(chars) => {
                for char in chars {
                    print!("{}", char);
                }
            }
            Value::Char(char) => print!("{char}"),
            Value::List(elements) => {
                print!("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    elem.print();
                }
                print!("]");
            }
            Value::Map(pairs) => {
                print!("{{");
                for (i, (key, value)) in pairs.iter().enumerate() {
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
        }
    }
}
