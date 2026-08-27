use crate::native::{NativeCall, NativeFunction as Function, NativeRegistry, NativeSignature as Signature, NativeSpace as Space};
use crate::{val, Data, Result, Ty, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Json",
        functions: &[
            Function {
                name: "parse",
                call: parse,
            },
            Function {
                name: "stringify",
                call: stringify,
            },
            Function {
                name: "pretty",
                call: pretty,
            },
            Function {
                name: "get",
                call: get,
            },
            Function {
                name: "at",
                call: at,
            },
            Function {
                name: "length",
                call: length,
            },
            Function {
                name: "kind",
                call: kind,
            },
            Function {
                name: "as_string",
                call: as_string,
            },
            Function {
                name: "as_int",
                call: as_int,
            },
            Function {
                name: "as_float",
                call: as_float,
            },
            Function {
                name: "as_bool",
                call: as_bool,
            },
            Function {
                name: "is_null",
                call: is_null,
            },
            Function { name: "string", call: json_string },
            Function { name: "int", call: json_int },
            Function { name: "float", call: json_float },
            Function { name: "bool", call: json_bool },
            Function { name: "null", call: json_null },
            Function { name: "array", call: json_array },
            Function { name: "object", call: json_object },
            Function { name: "strings", call: json_strings },
        ],
        signatures: || vec![
            Signature::exact("parse", vec![Ty::String], Ty::Json),
            Signature::exact("stringify", vec![Ty::Json], Ty::String),
            Signature::exact("pretty", vec![Ty::Json, Ty::Int], Ty::String),
            Signature::exact("get", vec![Ty::Json, Ty::String], Ty::Perchance(Box::new(Ty::Json))),
            Signature::exact("at", vec![Ty::Json, Ty::Int], Ty::Perchance(Box::new(Ty::Json))),
            Signature::exact("length", vec![Ty::Json], Ty::Int),
            Signature::exact("kind", vec![Ty::Json], Ty::String),
            Signature::exact("as_string", vec![Ty::Json], Ty::Perchance(Box::new(Ty::String))),
            Signature::exact("as_int", vec![Ty::Json], Ty::Perchance(Box::new(Ty::Int))),
            Signature::exact("as_float", vec![Ty::Json], Ty::Perchance(Box::new(Ty::Float))),
            Signature::exact("as_bool", vec![Ty::Json], Ty::Perchance(Box::new(Ty::Bool))),
            Signature::exact("is_null", vec![Ty::Json], Ty::Bool),
            Signature::exact("string", vec![Ty::String], Ty::Json),
            Signature::exact("int", vec![Ty::Int], Ty::Json),
            Signature::exact("float", vec![Ty::Float], Ty::Json),
            Signature::exact("bool", vec![Ty::Bool], Ty::Json),
            Signature::exact("null", vec![], Ty::Json),
            Signature::exact("array", vec![Ty::List(Box::new(Ty::Json))], Ty::Json),
            Signature::exact("object", vec![Ty::Map(Box::new(Ty::String), Box::new(Ty::Json))], Ty::Json),
            Signature::exact("strings", vec![Ty::Map(Box::new(Ty::String), Box::new(Ty::String))], Ty::Json),
        ],
    });
}

fn parse(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.parse")?;
    let value = serde_json::from_str(call.string(0, "Json.parse")?)
        .map_err(|error| call.error(format!("Json.parse failed: {error}")))?;
    Ok(call.json_value(value))
}

fn stringify(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.stringify")?;
    Ok(call.string_value(call.json(0, "Json.stringify")?.to_string()))
}

pub(crate) fn pretty_string(
    value: &serde_json::Value,
    spaces: i64,
) -> std::result::Result<String, String> {
    if !(0..=16).contains(&spaces) {
        return Err("indentation must be between 0 and 16 spaces".into());
    }
    if spaces == 0 {
        return Ok(value.to_string());
    }
    let standard = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    if spaces == 2 {
        return Ok(standard);
    }
    let indentation = " ".repeat(spaces as usize);
    Ok(standard
        .lines()
        .map(|line| {
            let depth = line.len() - line.trim_start_matches(' ').len();
            format!("{}{}", indentation.repeat(depth / 2), &line[depth..])
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn pretty(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Json.pretty")?;
    let spaces = call.int(1, "Json.pretty")?;
    let output = pretty_string(call.json(0, "Json.pretty")?, spaces)
        .map_err(|error| call.error(format!("Json.pretty failed: {error}")))?;
    Ok(call.string_value(output))
}

fn optional_json(value: Option<serde_json::Value>) -> Value {
    val(
        Ty::Perchance(Box::new(Ty::Json)),
        value.map(Data::Json).unwrap_or(Data::Naught),
    )
}

fn get(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Json.get")?;
    let key = call.string(1, "Json.get")?;
    let object = call
        .json(0, "Json.get")?
        .as_object()
        .ok_or_else(|| call.error("Json.get expects a JSON object"))?;
    Ok(optional_json(object.get(key).cloned()))
}

fn at(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Json.at")?;
    let index = call.int(1, "Json.at")?;
    let array = call
        .json(0, "Json.at")?
        .as_array()
        .ok_or_else(|| call.error("Json.at expects a JSON array"))?;
    let value = usize::try_from(index)
        .ok()
        .and_then(|index| array.get(index))
        .cloned();
    Ok(optional_json(value))
}

fn length(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.length")?;
    let length = match call.json(0, "Json.length")? {
        serde_json::Value::Array(values) => values.len(),
        serde_json::Value::Object(values) => values.len(),
        serde_json::Value::String(value) => value.chars().count(),
        _ => return Err(call.error("Json.length expects an array, object, or string")),
    };
    Ok(call.int_value(length as i64))
}

fn kind(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.kind")?;
    let kind = match call.json(0, "Json.kind")? {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    };
    Ok(call.string_value(kind))
}

fn optional_scalar(ty: Ty, data: Option<Data>) -> Value {
    val(Ty::Perchance(Box::new(ty)), data.unwrap_or(Data::Naught))
}

fn as_string(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.as_string")?;
    Ok(optional_scalar(
        Ty::String,
        call.json(0, "Json.as_string")?
            .as_str()
            .map(|value| Data::String(value.into())),
    ))
}

fn as_int(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.as_int")?;
    Ok(optional_scalar(
        Ty::Int,
        call.json(0, "Json.as_int")?.as_i64().map(Data::Int),
    ))
}

fn as_float(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.as_float")?;
    Ok(optional_scalar(
        Ty::Float,
        call.json(0, "Json.as_float")?.as_f64().map(Data::Float),
    ))
}

fn as_bool(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.as_bool")?;
    Ok(optional_scalar(
        Ty::Bool,
        call.json(0, "Json.as_bool")?.as_bool().map(Data::Bool),
    ))
}

fn is_null(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.is_null")?;
    Ok(call.bool_value(call.json(0, "Json.is_null")?.is_null()))
}

fn json_string(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.string")?;
    Ok(call.json_value(serde_json::Value::String(call.string(0, "Json.string")?.into())))
}

fn json_int(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.int")?;
    Ok(call.json_value(serde_json::Value::Number(call.int(0, "Json.int")?.into())))
}

fn json_float(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.float")?;
    let number = serde_json::Number::from_f64(call.float(0, "Json.float")?)
        .ok_or_else(|| call.error("Json.float cannot represent a non-finite value"))?;
    Ok(call.json_value(serde_json::Value::Number(number)))
}

fn json_bool(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.bool")?;
    Ok(call.json_value(serde_json::Value::Bool(call.bool(0, "Json.bool")?)))
}

fn json_null(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Json.null")?;
    Ok(call.json_value(serde_json::Value::Null))
}

fn json_array(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.array")?;
    let Data::List(values) = &call.value(0, "Json.array")?.data else {
        unreachable!()
    };
    let values = values
        .borrow()
        .iter()
        .map(|value| match &value.data {
            Data::Json(value) => Ok(value.clone()),
            _ => Err(call.error("Json.array expects list[json]")),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(call.json_value(serde_json::Value::Array(values)))
}

fn json_object(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.object")?;
    let Data::Map(values) = &call.value(0, "Json.object")?.data else {
        unreachable!()
    };
    let values = values
        .borrow()
        .iter()
        .map(|(key, value)| match &value.data {
            Data::Json(value) => Ok((map_string_key(key).into(), value.clone())),
            _ => Err(call.error("Json.object expects map[string, json]")),
        })
        .collect::<Result<serde_json::Map<_, _>>>()?;
    Ok(call.json_value(serde_json::Value::Object(values)))
}

fn json_strings(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Json.strings")?;
    let Data::Map(values) = &call.value(0, "Json.strings")?.data else {
        unreachable!()
    };
    let values = values
        .borrow()
        .iter()
        .map(|(key, value)| match &value.data {
            Data::String(value) => Ok((
                map_string_key(key).into(),
                serde_json::Value::String(value.clone()),
            )),
            _ => Err(call.error("Json.strings expects map[string, string]")),
        })
        .collect::<Result<serde_json::Map<_, _>>>()?;
    Ok(call.json_value(serde_json::Value::Object(values)))
}

fn map_string_key(key: &str) -> &str {
    key.strip_prefix("t:").unwrap_or(key)
}
