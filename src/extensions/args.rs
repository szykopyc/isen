use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use crate::native::{
    program_arguments, program_keywords, NativeCall, NativeFunction as Function, NativeRegistry,
    NativeSignature as Signature, NativeSpace as Space,
};
use crate::{val, Data, Result, Ty, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Args",
        functions: &[
            Function {
                name: "all",
                call: args_all,
            },
            Function {
                name: "get",
                call: args_get,
            },
        ],
        signatures: || vec![
            Signature::exact("all", vec![], Ty::List(Box::new(Ty::String))),
            Signature::exact("get", vec![Ty::Int], Ty::Perchance(Box::new(Ty::String))),
        ],
    });
    registry.add(Space {
        name: "Kwargs",
        functions: &[
            Function {
                name: "all",
                call: kwargs_all,
            },
            Function {
                name: "get",
                call: kwargs_get,
            },
            Function {
                name: "has",
                call: kwargs_has,
            },
        ],
        signatures: || vec![
            Signature::exact("all", vec![], Ty::Map(Box::new(Ty::String), Box::new(Ty::String))),
            Signature::exact("get", vec![Ty::String], Ty::Perchance(Box::new(Ty::String))),
            Signature::exact("has", vec![Ty::String], Ty::Bool),
        ],
    });
}

fn optional_string(value: Option<String>) -> Value {
    val(
        Ty::Perchance(Box::new(Ty::String)),
        value.map(Data::String).unwrap_or(Data::Naught),
    )
}

fn args_all(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Args.all")?;
    let values = program_arguments()
        .into_iter()
        .map(|argument| val(Ty::String, Data::String(argument)))
        .collect();
    Ok(val(
        Ty::List(Box::new(Ty::String)),
        Data::List(Rc::new(RefCell::new(values))),
    ))
}

fn args_get(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Args.get")?;
    let index = call.int(0, "Args.get")?;
    let value = usize::try_from(index)
        .ok()
        .and_then(|index| program_arguments().get(index).cloned());
    Ok(optional_string(value))
}

fn kwargs_all(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Kwargs.all")?;
    let values = program_keywords()
        .into_iter()
        .map(|(key, value)| (format!("t:{key}"), val(Ty::String, Data::String(value))))
        .collect::<BTreeMap<_, _>>();
    Ok(val(
        Ty::Map(Box::new(Ty::String), Box::new(Ty::String)),
        Data::Map(Rc::new(RefCell::new(values))),
    ))
}

fn kwargs_get(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Kwargs.get")?;
    let name = call.string(0, "Kwargs.get")?;
    Ok(optional_string(program_keywords().get(name).cloned()))
}

fn kwargs_has(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Kwargs.has")?;
    Ok(call.bool_value(program_keywords().contains_key(call.string(0, "Kwargs.has")?)))
}
