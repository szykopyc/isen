use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::{self, Write},
    rc::Rc,
};

use crate::native::{
    next_random_u64, seed_random, NativeCall, NativeExpected as Expected,
    NativeFunction as Function, NativeProduced as Produced, NativeRegistry,
    NativeSignature as Signature, NativeSpace as Space,
};
use crate::*;

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Random",
        functions: &[
            Function {
                name: "int",
                call: random_int,
            },
            Function {
                name: "float",
                call: random_float,
            },
            Function {
                name: "seed",
                call: random_seed,
            },
        ],
        signatures: || vec![
            Signature::exact("int", vec![Ty::Int, Ty::Int], Ty::Int),
            Signature::exact("float", vec![Ty::Float, Ty::Float], Ty::Float),
            Signature::exact("seed", vec![Ty::Int], Ty::Unit),
        ],
    });
    registry.add(Space {
        name: "Input",
        functions: &[Function {
            name: "line",
            call: input_line,
        }],
        signatures: || vec![Signature::exact("line", vec![Ty::String], Ty::String)],
    });
    registry.add(Space {
        name: "String",
        functions: &[
            Function {
                name: "tokens",
                call: text_tokens,
            },
            Function {
                name: "paragraph_tokens",
                call: text_paragraph_tokens,
            },
            Function {
                name: "lower",
                call: text_lower,
            },
            Function {
                name: "slice",
                call: text_slice,
            },
            Function {
                name: "split",
                call: text_split,
            },
            Function {
                name: "find",
                call: text_find,
            },
            Function {
                name: "join",
                call: text_join,
            },
            Function {
                name: "show",
                call: text_show,
            },
        ],
        signatures: || vec![
            Signature::exact("tokens", vec![Ty::String], Ty::List(Box::new(Ty::String))),
            Signature::exact("paragraph_tokens", vec![Ty::String], Ty::List(Box::new(Ty::String))),
            Signature::exact("lower", vec![Ty::String], Ty::String),
            Signature::exact("slice", vec![Ty::String, Ty::Int, Ty::Int], Ty::String),
            Signature::exact("split", vec![Ty::String, Ty::String], Ty::List(Box::new(Ty::String))),
            Signature::exact("find", vec![Ty::String, Ty::String], Ty::Perchance(Box::new(Ty::Int))),
            Signature::exact("join", vec![Ty::List(Box::new(Ty::String)), Ty::String], Ty::String),
            Signature::custom("show", vec![Expected::Any], Produced::Exact(Ty::String)),
        ],
    });
    registry.add(Space {
        name: "File",
        functions: &[
            Function {
                name: "read",
                call: file_read,
            },
            Function {
                name: "write",
                call: file_write,
            },
            Function {
                name: "append",
                call: file_append,
            },
            Function {
                name: "lines",
                call: file_lines,
            },
            Function {
                name: "make_dir",
                call: file_make_dir,
            },
            Function {
                name: "text_files",
                call: file_text_files,
            },
        ],
        signatures: || vec![
            Signature::exact("read", vec![Ty::String], Ty::String),
            Signature::exact("write", vec![Ty::String, Ty::String], Ty::Unit),
            Signature::exact("append", vec![Ty::String, Ty::String], Ty::Unit),
            Signature::exact("lines", vec![Ty::String], Ty::List(Box::new(Ty::String))),
            Signature::exact("make_dir", vec![Ty::String], Ty::Unit),
            Signature::exact("text_files", vec![Ty::String], Ty::List(Box::new(Ty::String))),
        ],
    });
    registry.add(Space {
        name: "List",
        functions: &[
            Function { name: "push", call: list_push },
            Function { name: "append", call: list_append },
            Function { name: "pop", call: list_pop },
            Function { name: "shift", call: list_shift },
        ],
        signatures: || vec![
            Signature::custom("push", vec![Expected::List, Expected::Any], Produced::SameAs(0)),
            Signature::custom("append", vec![Expected::List, Expected::Any], Produced::Exact(Ty::Unit)),
            Signature::custom("pop", vec![Expected::List], Produced::OptionalListElement(0)),
            Signature::custom("shift", vec![Expected::List], Produced::OptionalListElement(0)),
        ],
    });
    registry.add(Space {
        name: "Stack",
        functions: &[Function { name: "push", call: list_append }, Function { name: "pop", call: list_pop }],
        signatures: worklist_signatures,
    });
    registry.add(Space {
        name: "Queue",
        functions: &[Function { name: "push", call: list_append }, Function { name: "pop", call: list_shift }],
        signatures: worklist_signatures,
    });
    registry.add(Space {
        name: "Range",
        functions: &[Function { name: "until", call: range_until }, Function { name: "between", call: range_between }, Function { name: "step", call: range_step }],
        signatures: || vec![
            Signature::exact("until", vec![Ty::Int], Ty::List(Box::new(Ty::Int))),
            Signature::exact("between", vec![Ty::Int, Ty::Int], Ty::List(Box::new(Ty::Int))),
            Signature::exact("step", vec![Ty::Int, Ty::Int, Ty::Int], Ty::List(Box::new(Ty::Int))),
        ],
    });
    registry.add(Space {
        name: "Ordering",
        functions: &[Function { name: "less", call: ordering_less }, Function { name: "compare", call: ordering_compare }],
        signatures: || vec![
            Signature::custom("less", vec![Expected::Ordered, Expected::SameAs(0)], Produced::Exact(Ty::Bool)),
            Signature::custom("compare", vec![Expected::Ordered, Expected::SameAs(0)], Produced::Exact(Ty::Int)),
        ],
    });
    registry.add(Space {
        name: "Map",
        functions: &[
            Function {
                name: "string_int",
                call: map_string_int,
            },
            Function {
                name: "has",
                call: map_has,
            },
            Function { name: "get", call: map_get },
            Function { name: "keys", call: map_keys },
            Function {
                name: "top_string_int",
                call: map_top_string_int,
            },
        ],
        signatures: || vec![
            Signature::exact("string_int", vec![], Ty::Map(Box::new(Ty::String), Box::new(Ty::Int))),
            Signature::custom("has", vec![Expected::Map, Expected::Any], Produced::Exact(Ty::Bool)),
            Signature::custom("get", vec![Expected::Map, Expected::Any], Produced::OptionalMapValue(0)),
            Signature::custom("keys", vec![Expected::Map], Produced::MapKeys(0)),
            Signature::exact("top_string_int", vec![Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)), Ty::Int], Ty::List(Box::new(Ty::String))),
        ],
    });
}

fn worklist_signatures() -> Vec<Signature> {
    vec![
        Signature::custom("push", vec![Expected::List, Expected::Any], Produced::Exact(Ty::Unit)),
        Signature::custom("pop", vec![Expected::List], Produced::OptionalListElement(0)),
    ]
}

fn random_int(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Random.int")?;
    let low = call.int(0, "Random.int")?;
    let high = call.int(1, "Random.int")?;
    if low > high {
        return Err(call.error("Random.int low bound cannot exceed high bound"));
    }
    let width = high
        .checked_sub(low)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| call.error("Random.int range is too wide"))? as u64;
    Ok(call.int_value(low + (next_random_u64() % width) as i64))
}

fn random_float(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Random.float")?;
    let low = call.float(0, "Random.float")?;
    let high = call.float(1, "Random.float")?;
    if low > high {
        return Err(call.error("Random.float low bound cannot exceed high bound"));
    }
    let unit = next_random_u64() as f64 / u64::MAX as f64;
    Ok(call.float_value(low + (high - low) * unit))
}

fn random_seed(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Random.seed")?;
    seed_random(call.int(0, "Random.seed")?);
    Ok(call.unit_value())
}

fn input_line(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Input.line")?;
    print!("{}", call.string(0, "Input.line")?);
    io::stdout()
        .flush()
        .map_err(|e| call.error(format!("Input.line failed: {e}")))?;
    let mut input = String::new();
    let read = io::stdin()
        .read_line(&mut input)
        .map_err(|e| call.error(format!("Input.line failed: {e}")))?;
    if read == 0 {
        input = "/quit".into();
    } else {
        input.truncate(input.trim_end_matches(['\r', '\n']).len());
    }
    Ok(call.string_value(input))
}

fn words(string: &str, paragraphs: bool) -> Vec<Value> {
    let mut output = Vec::new();
    let mut boundary = false;
    for line in string.lines() {
        if paragraphs && line.trim().is_empty() {
            boundary = !output.is_empty();
            continue;
        }
        if boundary {
            output.push(val(Ty::String, Data::String("<paragraph>".into())));
            boundary = false;
        }
        let mut word = String::new();
        for character in line.chars() {
            if character.is_alphanumeric() || character == '\'' {
                word.push(character);
            } else {
                if !word.is_empty() {
                    output.push(val(Ty::String, Data::String(std::mem::take(&mut word))));
                }
                if !character.is_whitespace() {
                    output.push(val(Ty::String, Data::String(character.to_string())));
                }
            }
        }
        if !word.is_empty() {
            output.push(val(Ty::String, Data::String(word)));
        }
    }
    output
}
fn text_list(values: Vec<Value>) -> Value {
    val(
        Ty::List(Box::new(Ty::String)),
        Data::List(Rc::new(RefCell::new(values))),
    )
}
fn text_tokens(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "String.tokens")?;
    Ok(text_list(words(call.string(0, "String.tokens")?, false)))
}
fn text_paragraph_tokens(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "String.paragraph_tokens")?;
    Ok(text_list(words(
        call.string(0, "String.paragraph_tokens")?,
        true,
    )))
}
fn text_lower(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "String.lower")?;
    Ok(call.string_value(call.string(0, "String.lower")?.to_lowercase()))
}

fn text_slice(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(3, "String.slice")?;
    let string = call.string(0, "String.slice")?;
    let start = call.int(1, "String.slice")?;
    let end = call.int(2, "String.slice")?;
    let length = string.chars().count() as i64;
    if start < 0 || end < start || end > length {
        return Err(call.error("String.slice expects 0 <= start <= end <= size(string)"));
    }
    Ok(call.string_value(
        string
            .chars()
            .skip(start as usize)
            .take((end - start) as usize)
            .collect::<String>(),
    ))
}

fn text_split(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "String.split")?;
    let string = call.string(0, "String.split")?;
    let separator = call.string(1, "String.split")?;
    let pieces: Vec<String> = if separator.is_empty() {
        string
            .chars()
            .map(|character| character.to_string())
            .collect()
    } else {
        string.split(separator).map(str::to_owned).collect()
    };
    Ok(text_list(
        pieces
            .into_iter()
            .map(|piece| val(Ty::String, Data::String(piece)))
            .collect(),
    ))
}

fn text_find(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "String.find")?;
    let string = call.string(0, "String.find")?;
    let needle = call.string(1, "String.find")?;
    let ty = Ty::Perchance(Box::new(Ty::Int));
    let data = if let Some(byte_index) = string.find(needle) {
        Data::Int(string[..byte_index].chars().count() as i64)
    } else {
        Data::Naught
    };
    Ok(val(ty, data))
}

fn text_join(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "String.join")?;
    let Data::List(values) = &call.value(0, "String.join")?.data else {
        return Err(call.error("String.join expects list[string] as argument 1"));
    };
    let mut pieces = Vec::with_capacity(values.borrow().len());
    for value in values.borrow().iter() {
        let Data::String(string) = &value.data else {
            return Err(call.error("String.join expects list[string] as argument 1"));
        };
        pieces.push(string.clone());
    }
    Ok(call.string_value(pieces.join(call.string(1, "String.join")?)))
}

fn text_show(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "String.show")?;
    Ok(call.string_value(call.shown(0, "String.show")?))
}

fn file_read(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "File.read")?;
    let string = fs::read_to_string(call.string(0, "File.read")?)
        .map_err(|e| call.error(format!("File.read failed: {e}")))?;
    Ok(call.string_value(string))
}
fn file_write(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "File.write")?;
    let path = call.string(0, "File.write")?;
    let temporary = format!("{path}.tmp");
    fs::write(&temporary, call.string(1, "File.write")?)
        .map_err(|e| call.error(format!("File.write failed: {e}")))?;
    fs::rename(&temporary, path).map_err(|e| call.error(format!("File.write failed: {e}")))?;
    Ok(call.unit_value())
}
fn file_append(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "File.append")?;
    let path = call.string(0, "File.append")?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| call.error(format!("File.append failed: {error}")))?;
    file.write_all(call.string(1, "File.append")?.as_bytes())
        .map_err(|error| call.error(format!("File.append failed: {error}")))?;
    Ok(call.unit_value())
}
fn file_lines(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "File.lines")?;
    let string = fs::read_to_string(call.string(0, "File.lines")?)
        .map_err(|e| call.error(format!("File.lines failed: {e}")))?;
    Ok(text_list(
        string
            .lines()
            .map(|s| val(Ty::String, Data::String(s.into())))
            .collect(),
    ))
}
fn file_make_dir(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "File.make_dir")?;
    fs::create_dir_all(call.string(0, "File.make_dir")?)
        .map_err(|e| call.error(format!("File.make_dir failed: {e}")))?;
    Ok(call.unit_value())
}
fn file_text_files(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "File.text_files")?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(call.string(0, "File.text_files")?)
        .map_err(|e| call.error(format!("File.text_files failed: {e}")))?
    {
        let path = entry
            .map_err(|e| call.error(format!("File.text_files failed: {e}")))?
            .path();
        if path.is_file()
            && path
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("txt"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(text_list(
        paths
            .into_iter()
            .map(|p| val(Ty::String, Data::String(p.to_string_lossy().into_owned())))
            .collect(),
    ))
}

fn list_push(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "List.push")?;
    let list = call.value(0, "List.push")?.clone();
    let item = call.value(1, "List.push")?.clone();
    let Ty::List(element) = list.ty.clone() else {
        return Err(call.error("List.push first argument must be list"));
    };
    let actual = item.ty.clone();
    let Some(item) = conform(item, &element) else {
        return Err(call.error(format!("List.push expects {element}, got {actual}")));
    };
    let Data::List(items) = list.data else {
        unreachable!()
    };
    let mut values = items.borrow().clone();
    values.push(item);
    Ok(val(
        Ty::List(element),
        Data::List(Rc::new(RefCell::new(values))),
    ))
}
fn list_append(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "List.append")?;
    let list = call.value(0, "List.append")?;
    let item = call.value(1, "List.append")?.clone();
    let Ty::List(element) = &list.ty else { return Err(call.error("first argument must be list")); };
    let actual = item.ty.clone();
    let Some(item) = conform(item, element) else {
        return Err(call.error(format!("list expects {element}, got {actual}")));
    };
    let Data::List(items) = &list.data else { unreachable!() };
    items.borrow_mut().push(item);
    Ok(call.unit_value())
}
fn list_pop(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "List.pop")?;
    let list = call.value(0, "List.pop")?;
    let Ty::List(element) = &list.ty else { return Err(call.error("argument must be list")); };
    let Data::List(items) = &list.data else { unreachable!() };
    Ok(match items.borrow_mut().pop() {
        Some(item) => val(Ty::Perchance(element.clone()), item.data),
        None => val(Ty::Perchance(element.clone()), Data::Naught),
    })
}
fn list_shift(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "List.shift")?;
    let list = call.value(0, "List.shift")?;
    let Ty::List(element) = &list.ty else { return Err(call.error("argument must be list")); };
    let Data::List(items) = &list.data else { unreachable!() };
    Ok(if items.borrow().is_empty() {
        val(Ty::Perchance(element.clone()), Data::Naught)
    } else {
        let item = items.borrow_mut().remove(0);
        val(Ty::Perchance(element.clone()), item.data)
    })
}
fn range_until(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Range.until")?;
    range_values(0, call.int(0, "Range.until")?, 1, &call)
}
fn range_between(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Range.between")?;
    range_values(call.int(0, "Range.between")?, call.int(1, "Range.between")?, 1, &call)
}
fn range_step(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(3, "Range.step")?;
    range_values(call.int(0, "Range.step")?, call.int(1, "Range.step")?, call.int(2, "Range.step")?, &call)
}
fn range_values(start: i64, stop: i64, step: i64, call: &NativeCall<'_>) -> Result<Value> {
    if step == 0 { return Err(call.error("range step cannot be zero")); }
    let mut values = Vec::new();
    let mut value = start;
    while (step > 0 && value < stop) || (step < 0 && value > stop) {
        values.push(val(Ty::Int, Data::Int(value)));
        value = value.checked_add(step).ok_or_else(|| call.error("range overflow"))?;
    }
    Ok(val(Ty::List(Box::new(Ty::Int)), Data::List(Rc::new(RefCell::new(values)))))
}
fn ordering_value(left: &Value, right: &Value, call: &NativeCall<'_>) -> Result<std::cmp::Ordering> {
    if !same(&left.ty, &right.ty) { return Err(call.error("ordered values must have the same type")); }
    match (&left.data, &right.data) {
        (Data::Int(left), Data::Int(right)) => Ok(left.cmp(right)),
        (Data::String(left), Data::String(right)) => Ok(left.cmp(right)),
        (Data::Float(left), Data::Float(right)) => left.partial_cmp(right).ok_or_else(|| call.error("NaN values are not orderable")),
        _ => Err(call.error(format!("{} has no ordering", left.ty))),
    }
}
fn ordering_less(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Ordering.less")?;
    Ok(call.bool_value(ordering_value(call.value(0, "Ordering.less")?, call.value(1, "Ordering.less")?, &call)?.is_lt()))
}
fn ordering_compare(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Ordering.compare")?;
    let result = match ordering_value(call.value(0, "Ordering.compare")?, call.value(1, "Ordering.compare")?, &call)? {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    Ok(call.int_value(result))
}
fn map_string_int(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Map.string_int")?;
    Ok(val(
        Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)),
        Data::Map(Rc::new(RefCell::new(BTreeMap::new()))),
    ))
}
fn map_has(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Map.has")?;
    let map = call.value(0, "Map.has")?;
    let lookup = call.value(1, "Map.has")?;
    let Ty::Map(key_type, _) = &map.ty else {
        return Err(call.error("Map.has first argument must be map"));
    };
    if !same(key_type, &lookup.ty) {
        return Err(call.error(format!("map key expects {key_type}, got {}", lookup.ty)));
    }
    let Data::Map(entries) = &map.data else {
        unreachable!()
    };
    Ok(call.bool_value(entries.borrow().contains_key(&key(lookup, call.line())?)))
}
fn map_get(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Map.get")?;
    let map = call.value(0, "Map.get")?;
    let lookup = call.value(1, "Map.get")?;
    let Ty::Map(key_type, value_type) = &map.ty else { return Err(call.error("first argument must be map")); };
    if !same(key_type, &lookup.ty) { return Err(call.error(format!("map key expects {key_type}, got {}", lookup.ty))); }
    let Data::Map(entries) = &map.data else { unreachable!() };
    Ok(match entries.borrow().get(&key(lookup, call.line())?).cloned() {
        Some(value) => val(Ty::Perchance(value_type.clone()), value.data),
        None => val(Ty::Perchance(value_type.clone()), Data::Naught),
    })
}
fn map_keys(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Map.keys")?;
    let map = call.value(0, "Map.keys")?;
    let Ty::Map(key_type, _) = &map.ty else { return Err(call.error("argument must be map")); };
    let Data::Map(entries) = &map.data else { unreachable!() };
    let values = entries.borrow().keys().map(|encoded| decode_map_key(encoded, key_type, call.line())).collect::<Result<Vec<_>>>()?;
    Ok(val(Ty::List(key_type.clone()), Data::List(Rc::new(RefCell::new(values)))))
}
fn map_top_string_int(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Map.top_string_int")?;
    let map = call.value(0, "Map.top_string_int")?;
    let limit = call.int(1, "Map.top_string_int")?;
    if map.ty != Ty::Map(Box::new(Ty::String), Box::new(Ty::Int)) {
        return Err(call.error("Map.top_string_int expects map[string, int]"));
    }
    if limit < 0 {
        return Err(call.error("Map.top_string_int limit cannot be negative"));
    }
    let Data::Map(entries) = &map.data else {
        unreachable!()
    };
    let mut ranked = entries
        .borrow()
        .iter()
        .map(|(k, v)| {
            let Data::Int(n) = v.data else { unreachable!() };
            (n, k.strip_prefix("t:").unwrap_or(k).to_owned())
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(text_list(
        ranked
            .into_iter()
            .take(limit as usize)
            .map(|(_, w)| val(Ty::String, Data::String(w)))
            .collect(),
    ))
}
