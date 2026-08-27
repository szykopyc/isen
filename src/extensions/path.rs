use std::{
    cell::RefCell,
    env, fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::native::{NativeCall, NativeFunction as Function, NativeRegistry, NativeSignature as Signature, NativeSpace as Space};
use crate::{val, Data, Result, Ty, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Path",
        functions: &[
            Function {
                name: "current",
                call: current,
            },
            Function {
                name: "join",
                call: join,
            },
            Function {
                name: "exists",
                call: exists,
            },
            Function {
                name: "is_file",
                call: is_file,
            },
            Function {
                name: "is_dir",
                call: is_dir,
            },
            Function {
                name: "canonical",
                call: canonical,
            },
            Function {
                name: "list",
                call: list,
            },
            Function {
                name: "name",
                call: name,
            },
            Function {
                name: "parent",
                call: parent,
            },
        ],
        signatures: || vec![
            Signature::exact("current", vec![], Ty::String),
            Signature::exact("join", vec![Ty::String, Ty::String], Ty::String),
            Signature::exact("exists", vec![Ty::String], Ty::Bool),
            Signature::exact("is_file", vec![Ty::String], Ty::Bool),
            Signature::exact("is_dir", vec![Ty::String], Ty::Bool),
            Signature::exact("canonical", vec![Ty::String], Ty::String),
            Signature::exact("list", vec![Ty::String], Ty::List(Box::new(Ty::String))),
            Signature::exact("name", vec![Ty::String], Ty::Perchance(Box::new(Ty::String))),
            Signature::exact("parent", vec![Ty::String], Ty::Perchance(Box::new(Ty::String))),
        ],
    });
}

fn optional_string(value: Option<String>) -> Value {
    val(
        Ty::Perchance(Box::new(Ty::String)),
        value.map(Data::String).unwrap_or(Data::Naught),
    )
}

fn path_string(path: &Path, call: &NativeCall<'_>, function: &str) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| call.error(format!("{function} encountered a path that is not UTF-8")))
}

fn current(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Path.current")?;
    let path =
        env::current_dir().map_err(|error| call.error(format!("Path.current failed: {error}")))?;
    Ok(call.string_value(path_string(&path, &call, "Path.current")?))
}

fn join(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Path.join")?;
    let mut path = PathBuf::from(call.string(0, "Path.join")?);
    path.push(call.string(1, "Path.join")?);
    Ok(call.string_value(path_string(&path, &call, "Path.join")?))
}

fn exists(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.exists")?;
    Ok(call.bool_value(Path::new(call.string(0, "Path.exists")?).exists()))
}

fn is_file(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.is_file")?;
    Ok(call.bool_value(Path::new(call.string(0, "Path.is_file")?).is_file()))
}

fn is_dir(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.is_dir")?;
    Ok(call.bool_value(Path::new(call.string(0, "Path.is_dir")?).is_dir()))
}

fn canonical(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.canonical")?;
    let path = fs::canonicalize(call.string(0, "Path.canonical")?)
        .map_err(|error| call.error(format!("Path.canonical failed: {error}")))?;
    Ok(call.string_value(path_string(&path, &call, "Path.canonical")?))
}

fn list(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.list")?;
    let mut paths = fs::read_dir(call.string(0, "Path.list")?)
        .map_err(|error| call.error(format!("Path.list failed: {error}")))?
        .map(|entry| {
            let path = entry
                .map_err(|error| call.error(format!("Path.list failed: {error}")))?
                .path();
            path_string(&path, &call, "Path.list")
        })
        .collect::<Result<Vec<_>>>()?;
    paths.sort();
    let values = paths
        .into_iter()
        .map(|path| val(Ty::String, Data::String(path)))
        .collect();
    Ok(val(
        Ty::List(Box::new(Ty::String)),
        Data::List(Rc::new(RefCell::new(values))),
    ))
}

fn name(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.name")?;
    let value = match Path::new(call.string(0, "Path.name")?).file_name() {
        Some(name) => Some(
            name.to_str()
                .ok_or_else(|| call.error("Path.name encountered a path that is not UTF-8"))?
                .to_owned(),
        ),
        None => None,
    };
    Ok(optional_string(value))
}

fn parent(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Path.parent")?;
    let value = match Path::new(call.string(0, "Path.parent")?).parent() {
        Some(parent) => Some(path_string(parent, &call, "Path.parent")?),
        None => None,
    };
    Ok(optional_string(value))
}
