use std::{cell::RefCell, collections::BTreeMap, env, fs, io::ErrorKind, rc::Rc};

use crate::native::{NativeCall, NativeFunction as Function, NativeRegistry, NativeSignature as Signature, NativeSpace as Space};
use crate::{val, Data, Result, Ty, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Env",
        functions: &[
            Function {
                name: "get",
                call: get,
            },
            Function {
                name: "read",
                call: read,
            },
        ],
        signatures: || vec![
            Signature::exact("get", vec![Ty::String], Ty::Perchance(Box::new(Ty::String))),
            Signature::exact(
                "read",
                vec![Ty::String],
                Ty::Perchance(Box::new(Ty::Map(Box::new(Ty::String), Box::new(Ty::String)))),
            ),
        ],
    });
}

fn optional_string(value: Option<String>) -> Value {
    val(
        Ty::Perchance(Box::new(Ty::String)),
        value.map(Data::String).unwrap_or(Data::Naught),
    )
}

fn get(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Env.get")?;
    let name = call.string(0, "Env.get")?;
    match env::var(name) {
        Ok(value) => Ok(optional_string(Some(value))),
        Err(env::VarError::NotPresent) => Ok(optional_string(None)),
        Err(env::VarError::NotUnicode(_)) => {
            Err(call.error(format!("Env.get value for {name:?} is not UTF-8")))
        }
    }
}

fn read(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Env.read")?;
    let path = call.string(0, "Env.read")?;
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(val(
                Ty::Perchance(Box::new(Ty::Map(
                    Box::new(Ty::String),
                    Box::new(Ty::String),
                ))),
                Data::Naught,
            ));
        }
        Err(error) => return Err(call.error(format!("Env.read failed for {path:?}: {error}"))),
    };

    let mut values = BTreeMap::new();
    for (index, raw_line) in source.lines().enumerate() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim_start();
        }
        let Some((name, raw_value)) = line.split_once('=') else {
            return Err(call.error(format!(
                "Env.read {path:?}:{} expects NAME=VALUE",
                index + 1
            )));
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(call.error(format!(
                "Env.read {path:?}:{} has invalid variable name {name:?}",
                index + 1
            )));
        }
        let raw_value = raw_value.trim();
        let value = if raw_value.len() >= 2
            && ((raw_value.starts_with('"') && raw_value.ends_with('"'))
                || (raw_value.starts_with('\'') && raw_value.ends_with('\'')))
        {
            raw_value[1..raw_value.len() - 1].to_owned()
        } else {
            raw_value.to_owned()
        };
        if values
            .insert(format!("t:{name}"), val(Ty::String, Data::String(value)))
            .is_some()
        {
            return Err(call.error(format!(
                "Env.read {path:?}:{} repeats variable {name:?}",
                index + 1
            )));
        }
    }
    Ok(val(
        Ty::Perchance(Box::new(Ty::Map(
            Box::new(Ty::String),
            Box::new(Ty::String),
        ))),
        Data::Map(Rc::new(RefCell::new(values))),
    ))
}
