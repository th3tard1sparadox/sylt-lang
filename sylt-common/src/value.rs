use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::{ty::Type, upvalue::UpValue};

#[derive(Clone)]
#[derive(Deserialize, Serialize)]
pub enum Value {
    Ty(Type),
    Blob(Rc<RefCell<HashMap<String, Value>>>),
    Tuple(Rc<Vec<Value>>),
    List(Rc<RefCell<Vec<Value>>>),
    Set(Rc<RefCell<HashSet<Value>>>),
    Dict(Rc<RefCell<HashMap<Value, Value>>>),
    Float(f64),
    Int(i64),
    Bool(bool),
    String(Rc<String>),
    Function(Rc<Vec<Rc<RefCell<UpValue>>>>, Type, usize),
    ExternFunction(usize),
    Nil,
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(Rc::new(s.to_string()))
    }
}

impl From<&Type> for Value {
    fn from(ty: &Type) -> Self {
        match ty {

            Type::Unknown
            | Type::Invalid
            | Type::Generic(_)
            | Type::Union(_) => panic!("This type cannot be represented as a value!"),
            Type::Void => Value::Nil,
            Type::Blob(_, f) => Value::Blob(Rc::new(RefCell::new(
                f.iter().map(|(n, t)| (n.clone(), t.into())).collect()
            ))),
            Type::Tuple(fields) => Value::Tuple(Rc::new(fields.iter().map(Value::from).collect())),
            Type::List(v) => Value::List(Rc::new(RefCell::new(vec![Value::from(v.as_ref())]))),
            Type::Set(v) => {
                let mut s = HashSet::new();
                s.insert(Value::from(v.as_ref()));
                Value::Set(Rc::new(RefCell::new(s)))
            }
            Type::Dict(k, v) => {
                let mut s = HashMap::new();
                s.insert(Value::from(k.as_ref()), Value::from(v.as_ref()));
                Value::Dict(Rc::new(RefCell::new(s)))
            }
            Type::Int => Value::Int(1),
            Type::Float => Value::Float(1.0),
            Type::Bool => Value::Bool(true),
            Type::String => Value::String(Rc::new("".to_string())),
            Type::Function(a, r) => {
                Value::Function(Rc::new(Vec::new()), Type::Function(a.clone(), r.clone()), 0)
            }
            Type::ExternFunction(x) => Value::ExternFunction(*x),
            Type::Ty => Value::Ty(Type::Void),
        }
    }
}

impl From<Type> for Value {
    fn from(ty: Type) -> Self {
        Value::from(&ty)
    }
}

impl Debug for Value {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.safe_fmt(fmt, &mut HashSet::new())
    }
}

impl Display for Value {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(fmt, "{}", s),
            value => value.safe_fmt(fmt, &mut HashSet::new())
        }
    }
}

impl PartialEq<Value> for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a == b)
            }
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
            (Value::Dict(a), Value::Dict(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Float(a) => {
                // We have to limit the values, because
                // floats are wierd.
                assert!(a.is_finite());
                a.to_bits().hash(state);
            }
            Value::Int(a) => a.hash(state),
            Value::Bool(a) => a.hash(state),
            Value::String(a) => a.hash(state),
            Value::Tuple(a) => a.hash(state),
            Value::Nil => state.write_i8(0),
            _ => {}
        };
    }
}

impl Value {
    pub fn is_nil(&self) -> bool {
        matches!(self, Value::Nil)
    }

    pub fn unique_id(&self) -> usize {
        match self {
            Value::Ty(ty) => ty as *const _ as usize,
            Value::Float(f) => f as *const _ as usize,
            Value::Int(i) => i as *const _ as usize,
            Value::Bool(b) => b as *const _ as usize,
            Value::Blob(v) => Rc::as_ptr(v) as usize,
            Value::String(s) => Rc::as_ptr(s) as usize,
            Value::List(v) => Rc::as_ptr(v) as usize,
            Value::Set(v) => Rc::as_ptr(v) as usize,
            Value::Dict(v) => Rc::as_ptr(v) as usize,
            Value::Function(v, _, _) => Rc::as_ptr(v) as usize,
            Value::Tuple(v) => Rc::as_ptr(v) as usize,
            Value::Nil => 0,  // TODO(ed): This is not a valid pointer - right?
            Value::ExternFunction(slot) => slot + 2,
        }
    }

    /// Format the Value to a nice readable format while removing endless
    /// recursion.
    fn safe_fmt(
        &self,
        fmt: &mut std::fmt::Formatter<'_>,
        seen: &mut HashSet<usize>
    ) -> std::fmt::Result {
        match self {
            Value::Ty(ty) => write!(fmt, "<type \"{:?}\">", ty),
            Value::Blob(v) => {
                write!(fmt, "{} (0x{:x}) {{",
                    if let Some(Value::String(name)) = v.borrow().get("_name") {
                        name.as_str()
                    } else {
                        unreachable!("Got blob without a name")
                    },
                    self.unique_id()
                )?;
                if !seen.insert(self.unique_id()) {
                    return write!(fmt, "...}}");
                }
                let mut first = true;
                for e in v.borrow().iter() {
                    if e.0.starts_with("_") {
                        continue;
                    }
                    if !first {
                        write!(fmt, ", ")?;
                    }
                    write!(fmt, "{}", e.0)?;
                    write!(fmt, ": ")?;
                    e.1.safe_fmt(fmt, seen)?;
                    first = false;
                }
                if v.borrow().len() == 0 {
                    write!(fmt, ":")?;
                }
                write!(fmt, "}}")
            },
            Value::Float(f) => write!(fmt, "{:?}", f),
            Value::Int(i) => write!(fmt, "{}", i),
            Value::Bool(b) => write!(fmt, "{}", b),
            Value::String(s) => write!(fmt, "\"{}\"", s),
            Value::List(v) => {
                if !seen.insert(self.unique_id()) {
                    return write!(fmt, "[...]");
                }
                write!(fmt, "[")?;
                for (i, e) in v.borrow().iter().enumerate() {
                    if i != 0 {
                        write!(fmt, ", ")?;
                    }
                    e.safe_fmt(fmt, seen)?;
                }
                write!(fmt, "]")
            },
            Value::Tuple(v) => {
                write!(fmt, "(")?;
                for (i, e) in v.iter().enumerate() {
                    if i != 0 {
                        write!(fmt, ", ")?;
                    }
                    e.safe_fmt(fmt, seen)?;
                }
                if v.len() == 1 {
                    write!(fmt, ",")?
                }
                write!(fmt, ")")
            },
            Value::Set(v) => {
                if !seen.insert(self.unique_id()) {
                    return write!(fmt, "{{...}}");
                }
                write!(fmt, "{{")?;
                for (i, e) in v.borrow().iter().enumerate() {
                    if i != 0 {
                        write!(fmt, ", ")?;
                    }
                    e.safe_fmt(fmt, seen)?;
                }
                write!(fmt, "}}")
            },
            Value::Dict(v) => {
                if !seen.insert(self.unique_id()) {
                    return write!(fmt, "{{...}}");
                }
                write!(fmt, "{{")?;
                for (i, e) in v.borrow().iter().enumerate() {
                    if i != 0 {
                        write!(fmt, ", ")?;
                    }
                    e.0.safe_fmt(fmt, seen)?;
                    write!(fmt, ": ")?;
                    e.1.safe_fmt(fmt, seen)?;
                }
                if v.borrow().len() == 0 {
                    write!(fmt, ":")?;
                }
                write!(fmt, "}}")
            },
            Value::Function(_, ty, block) => {
                write!(fmt, "<fn #{} {:?}>", block, ty)
            },
            Value::ExternFunction(slot) => write!(fmt, "<extern fn {}>", slot),
            Value::Nil => write!(fmt, "nil"),
        }
    }
}
