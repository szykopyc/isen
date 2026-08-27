use crate::*;
pub(crate) fn register(registry: &mut crate::native::NativeRegistry) {
    use crate::native::{NativeExpected as Expected, NativeProduced as Produced, NativeRuntimeFunction as Function, NativeRuntimeSpace as Space, NativeSignature as Signature};
    registry.add_runtime(Space {
        name: "Array",
        functions: &[
            Function {
                name: "float",
                call: array_float,
            },
            Function {
                name: "int",
                call: array_int,
            },
            Function { name: "sized", call: array_sized },
            Function {
                name: "dot",
                call: array_dot,
            },
            Function {
                name: "axpy",
                call: array_axpy,
            },
            Function {
                name: "fill",
                call: array_fill,
            },
            Function {
                name: "copy",
                call: array_copy,
            },
            Function {
                name: "save",
                call: array_save,
            },
            Function {
                name: "load_float",
                call: array_load_float,
            },
        ],
        signatures: || {
            let arr_float = Ty::Arr(Box::new(Ty::Float));
            vec![
                Signature::exact("float", vec![Ty::Int, Ty::Float], arr_float.clone()),
                Signature::exact("int", vec![Ty::Int, Ty::Int], Ty::Arr(Box::new(Ty::Int))),
                Signature::custom("sized", vec![Expected::Exact(Ty::Int), Expected::Any], Produced::ArrayOfArgument(1)),
                Signature::exact("dot", vec![arr_float.clone(), Ty::Int, arr_float.clone(), Ty::Int, Ty::Int], Ty::Float),
                Signature::exact("axpy", vec![arr_float.clone(), Ty::Int, arr_float.clone(), Ty::Int, Ty::Int, Ty::Float], Ty::Unit),
                Signature::exact("fill", vec![arr_float.clone(), Ty::Int, Ty::Int, Ty::Float], Ty::Unit),
                Signature::exact("copy", vec![arr_float.clone(), Ty::Int, arr_float.clone(), Ty::Int, Ty::Int], Ty::Unit),
                Signature::exact("save", vec![Ty::String, arr_float.clone()], Ty::Unit),
                Signature::exact("load_float", vec![Ty::String], arr_float),
            ]
        },
    });
}

fn array_float(args: &[Expr], environment: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::new(
            line,
            "Array.float expects length and initial float value",
        ));
    }
    let values = evaluated_args(args, environment)?;
    let Data::Int(length) = &values[0].data else {
        return Err(Error::new(line, "Array.float length must be int"));
    };
    let Data::Float(initial) = &values[1].data else {
        return Err(Error::new(line, "Array.float initial value must be float"));
    };
    if *length < 0 {
        return Err(Error::new(line, "Array.float length cannot be negative"));
    }
    let items = (0..*length)
        .map(|_| val(Ty::Float, Data::Float(*initial)))
        .collect();
    Ok(val(
        Ty::Arr(Box::new(Ty::Float)),
        Data::Arr(Rc::new(RefCell::new(items))),
    ))
}

fn array_int(args: &[Expr], environment: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::new(line, "Array.int expects length and initial int"));
    }
    let values = evaluated_args(args, environment)?;
    let Data::Int(length) = &values[0].data else {
        return Err(Error::new(line, "Array.int length must be int"));
    };
    let Data::Int(initial) = &values[1].data else {
        return Err(Error::new(line, "Array.int initial value must be int"));
    };
    if *length < 0 {
        return Err(Error::new(line, "Array.int length cannot be negative"));
    }
    let items = (0..*length)
        .map(|_| val(Ty::Int, Data::Int(*initial)))
        .collect();
    Ok(val(
        Ty::Arr(Box::new(Ty::Int)),
        Data::Arr(Rc::new(RefCell::new(items))),
    ))
}

fn array_sized(args: &[Expr], environment: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 2 { return Err(Error::new(line, "Array.sized expects length and initial value")); }
    let values = evaluated_args(args, environment)?;
    let Data::Int(length) = values[0].data else { return Err(Error::new(line, "Array.sized length must be int")); };
    if length < 0 { return Err(Error::new(line, "Array.sized length cannot be negative")); }
    let initial = values[1].clone();
    Ok(val(Ty::Arr(Box::new(initial.ty.clone())), Data::Arr(Rc::new(RefCell::new(vec![initial; length as usize])))))
}

type ArrayParts = (
    Rc<RefCell<Vec<Value>>>,
    usize,
    Rc<RefCell<Vec<Value>>>,
    usize,
    usize,
);

pub(crate) fn array_parts(args: &[Expr], e: EnvRef, l: usize, count: usize) -> Result<ArrayParts> {
    if args.len() != count {
        return Err(Error::new(
            l,
            "Array operation received the wrong number of arguments",
        ));
    }
    let a = eval(&args[0], e.clone())?;
    let Data::Arr(a) = a.data else {
        return Err(Error::new(l, "Array operation expects float arrays"));
    };
    let ai = match eval(&args[1], e.clone())?.data {
        Data::Int(x) if x >= 0 => x as usize,
        _ => return Err(Error::new(l, "Array offsets must be non-negative int")),
    };
    let b = eval(&args[2], e.clone())?;
    let Data::Arr(b) = b.data else {
        return Err(Error::new(l, "Array operation expects float arrays"));
    };
    let bi = match eval(&args[3], e.clone())?.data {
        Data::Int(x) if x >= 0 => x as usize,
        _ => return Err(Error::new(l, "Array offsets must be non-negative int")),
    };
    let n = match eval(&args[4], e.clone())?.data {
        Data::Int(x) if x >= 0 => x as usize,
        _ => return Err(Error::new(l, "Array length must be non-negative int")),
    };
    if ai + n > a.borrow().len() || bi + n > b.borrow().len() {
        return Err(Error::new(l, "Array segment is out of bounds"));
    }
    Ok((a, ai, b, bi, n))
}
pub(crate) fn array_dot(args: &[Expr], e: EnvRef, l: usize) -> Result<Value> {
    let (a, ai, b, bi, n) = array_parts(args, e, l, 5)?;
    let a = a.borrow();
    let b = b.borrow();
    let mut sum = 0.0;
    for i in 0..n {
        match (&a[ai + i].data, &b[bi + i].data) {
            (Data::Float(x), Data::Float(y)) => sum += x * y,
            _ => return Err(Error::new(l, "Array.dot expects float arrays")),
        }
    }
    Ok(val(Ty::Float, Data::Float(sum)))
}
pub(crate) fn array_axpy(args: &[Expr], e: EnvRef, l: usize) -> Result<Value> {
    if args.len() != 6 {
        return Err(Error::new(
            l,
            "Array.axpy expects target, offset, source, offset, length, scale",
        ));
    }
    let scale = match eval(&args[5], e.clone())?.data {
        Data::Float(x) => x,
        _ => return Err(Error::new(l, "Array.axpy scale must be float")),
    };
    let (a, ai, b, bi, n) = array_parts(args, e, l, 6)?;
    let source = b.borrow();
    let mut target = a.borrow_mut();
    for i in 0..n {
        match (&mut target[ai + i].data, &source[bi + i].data) {
            (Data::Float(x), Data::Float(y)) => *x += scale * y,
            _ => return Err(Error::new(l, "Array.axpy expects float arrays")),
        }
    }
    Ok(val(Ty::Unit, Data::Unit))
}
pub(crate) fn array_fill(args: &[Expr], e: EnvRef, l: usize) -> Result<Value> {
    if args.len() != 4 {
        return Err(Error::new(
            l,
            "Array.fill expects target, offset, length, value",
        ));
    }
    let a = eval(&args[0], e.clone())?;
    let Data::Arr(a) = a.data else {
        return Err(Error::new(l, "Array.fill expects a float array"));
    };
    let start = match eval(&args[1], e.clone())?.data {
        Data::Int(x) if x >= 0 => x as usize,
        _ => return Err(Error::new(l, "Array.fill offset must be int")),
    };
    let n = match eval(&args[2], e.clone())?.data {
        Data::Int(x) if x >= 0 => x as usize,
        _ => return Err(Error::new(l, "Array.fill length must be int")),
    };
    let value = match eval(&args[3], e.clone())?.data {
        Data::Float(x) => x,
        _ => return Err(Error::new(l, "Array.fill value must be float")),
    };
    let mut a = a.borrow_mut();
    if start + n > a.len() {
        return Err(Error::new(l, "Array segment is out of bounds"));
    }
    for x in &mut a[start..start + n] {
        *x = val(Ty::Float, Data::Float(value));
    }
    Ok(val(Ty::Unit, Data::Unit))
}

pub(crate) fn array_copy(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    let (target, target_start, source, source_start, length) = array_parts(args, e, line, 5)?;
    let copied = source.borrow()[source_start..source_start + length].to_vec();
    let mut target = target.borrow_mut();
    target[target_start..target_start + length].clone_from_slice(&copied);
    Ok(val(Ty::Unit, Data::Unit))
}

pub(crate) fn array_save(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::new(line, "Array.save expects a path and array"));
    }
    let values = evaluated_args(args, e)?;
    let Data::String(path) = &values[0].data else {
        return Err(Error::new(line, "Array.save path must be string"));
    };
    let array = float_array(&values[1], line)?;
    let array = array.borrow();
    let mut bytes = Vec::with_capacity(16 + array.len() * 8);
    bytes.extend_from_slice(b"IDIOTF64");
    bytes.extend_from_slice(&(array.len() as u64).to_le_bytes());
    for item in array.iter() {
        let Data::Float(number) = &item.data else {
            return Err(Error::new(line, "Array.save expects arr[float]"));
        };
        bytes.extend_from_slice(&number.to_le_bytes());
    }
    let temporary = format!("{path}.tmp");
    fs::write(&temporary, bytes)
        .map_err(|err| Error::new(line, format!("Array.save failed: {err}")))?;
    fs::rename(&temporary, path)
        .map_err(|err| Error::new(line, format!("Array.save failed: {err}")))?;
    Ok(val(Ty::Unit, Data::Unit))
}

pub(crate) fn array_load_float(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::new(line, "Array.load_float expects one string path"));
    }
    let path = eval(&args[0], e)?;
    let Data::String(path) = path.data else {
        return Err(Error::new(line, "Array.load_float path must be string"));
    };
    let bytes = fs::read(&path)
        .map_err(|err| Error::new(line, format!("Array.load_float failed: {err}")))?;
    if bytes.len() < 16 || &bytes[..8] != b"IDIOTF64" {
        return Err(Error::new(line, "invalid Isen float-array file"));
    }
    let length = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let length =
        usize::try_from(length).map_err(|_| Error::new(line, "float-array file is too large"))?;
    let expected = length
        .checked_mul(8)
        .and_then(|body| body.checked_add(16))
        .ok_or_else(|| Error::new(line, "float-array file is too large"))?;
    if bytes.len() != expected {
        return Err(Error::new(line, "truncated or malformed float-array file"));
    }
    let mut values = Vec::with_capacity(length);
    for chunk in bytes[16..].chunks_exact(8) {
        let number = f64::from_le_bytes(chunk.try_into().unwrap());
        values.push(val(Ty::Float, Data::Float(number)));
    }
    Ok(val(
        Ty::Arr(Box::new(Ty::Float)),
        Data::Arr(Rc::new(RefCell::new(values))),
    ))
}

pub(crate) fn evaluated_args(args: &[Expr], e: EnvRef) -> Result<Vec<Value>> {
    args.iter().map(|arg| eval(arg, e.clone())).collect()
}

pub(crate) fn float_array(value: &Value, line: usize) -> Result<Rc<RefCell<Vec<Value>>>> {
    match (&value.ty, &value.data) {
        (Ty::Arr(element), Data::Arr(items)) if **element == Ty::Float => Ok(items.clone()),
        _ => Err(Error::new(line, "MLP kernels expect arr[float] values")),
    }
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn int_array(value: &Value, line: usize) -> Result<Rc<RefCell<Vec<Value>>>> {
    match (&value.ty, &value.data) {
        (Ty::Arr(element), Data::Arr(items)) if **element == Ty::Int => Ok(items.clone()),
        _ => Err(Error::new(line, "Array kernel expects an arr[int] value")),
    }
}
#[cfg(feature = "ml-kernels")]
pub(crate) fn int_value(value: &Value, line: usize, name: &str) -> Result<usize> {
    match &value.data {
        Data::Int(number) if *number >= 0 => Ok(*number as usize),
        _ => Err(Error::new(
            line,
            format!("{name} must be a non-negative int"),
        )),
    }
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn float_value(value: &Value, line: usize, name: &str) -> Result<f64> {
    match &value.data {
        Data::Float(number) => Ok(*number),
        _ => Err(Error::new(line, format!("{name} must be float"))),
    }
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_number(items: &[Value], index: usize, line: usize) -> Result<f64> {
    match items.get(index).map(|item| &item.data) {
        Some(Data::Float(number)) => Ok(*number),
        Some(_) => Err(Error::new(line, "MLP kernels expect arr[float] values")),
        None => Err(Error::new(line, "MLP array index is out of bounds")),
    }
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn set_array_number(
    items: &mut [Value],
    index: usize,
    number: f64,
    line: usize,
) -> Result<()> {
    let item = items
        .get_mut(index)
        .ok_or_else(|| Error::new(line, "MLP array index is out of bounds"))?;
    if !matches!(item.data, Data::Float(_)) {
        return Err(Error::new(line, "MLP kernels expect arr[float] values"));
    }
    item.data = Data::Float(number);
    Ok(())
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_mlp_forward(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 12 {
        return Err(Error::new(line, "ML.mlp_forward expects 12 arguments"));
    }
    let values = evaluated_args(args, e)?;
    let hidden = float_array(&values[0], line)?;
    let bias = float_array(&values[1], line)?;
    let left_matrix = float_array(&values[2], line)?;
    let left_embed = float_array(&values[3], line)?;
    let left = int_value(&values[4], line, "left word id")?;
    let middle_matrix = float_array(&values[5], line)?;
    let middle_embed = float_array(&values[6], line)?;
    let middle = int_value(&values[7], line, "middle word id")?;
    let right_matrix = float_array(&values[8], line)?;
    let right_embed = float_array(&values[9], line)?;
    let right = int_value(&values[10], line, "right word id")?;
    let width = int_value(&values[11], line, "MLP width")?;

    let bias = bias.borrow();
    let left_matrix = left_matrix.borrow();
    let left_embed = left_embed.borrow();
    let middle_matrix = middle_matrix.borrow();
    let middle_embed = middle_embed.borrow();
    let right_matrix = right_matrix.borrow();
    let right_embed = right_embed.borrow();
    let mut hidden = hidden.borrow_mut();
    for unit in 0..width {
        let mut sum = array_number(&bias, unit, line)?;
        for feature in 0..width {
            sum += array_number(&left_matrix, unit * width + feature, line)?
                * array_number(&left_embed, left * width + feature, line)?;
            sum += array_number(&middle_matrix, unit * width + feature, line)?
                * array_number(&middle_embed, middle * width + feature, line)?;
            sum += array_number(&right_matrix, unit * width + feature, line)?
                * array_number(&right_embed, right * width + feature, line)?;
        }
        set_array_number(&mut hidden, unit, sum.tanh(), line)?;
    }
    Ok(val(Ty::Unit, Data::Unit))
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_sampled_update(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 8 {
        return Err(Error::new(line, "ML.sampled_update expects 8 arguments"));
    }
    let values = evaluated_args(args, e)?;
    let output = float_array(&values[0], line)?;
    let output_bias = float_array(&values[1], line)?;
    let hidden = float_array(&values[2], line)?;
    let hidden_gradient = float_array(&values[3], line)?;
    let candidate = int_value(&values[4], line, "candidate")?;
    let label = float_value(&values[5], line, "label")?;
    let rate = float_value(&values[6], line, "learning rate")?;
    let width = int_value(&values[7], line, "MLP width")?;

    let hidden = hidden.borrow();
    let mut output = output.borrow_mut();
    let mut output_bias = output_bias.borrow_mut();
    let mut hidden_gradient = hidden_gradient.borrow_mut();
    let mut score = array_number(&output_bias, candidate, line)?;
    for unit in 0..width {
        score += array_number(&output, candidate * width + unit, line)?
            * array_number(&hidden, unit, line)?;
    }
    let probability = if score >= 0.0 {
        1.0 / (1.0 + (-score).exp())
    } else {
        let exp = score.exp();
        exp / (1.0 + exp)
    };
    let delta = probability - label;
    let bias = array_number(&output_bias, candidate, line)? - rate * delta;
    set_array_number(&mut output_bias, candidate, bias, line)?;
    for unit in 0..width {
        let index = candidate * width + unit;
        let weight = array_number(&output, index, line)?;
        let gradient = array_number(&hidden_gradient, unit, line)? + delta * weight;
        set_array_number(&mut hidden_gradient, unit, gradient, line)?;
        let updated = weight - rate * delta * array_number(&hidden, unit, line)?;
        set_array_number(&mut output, index, updated, line)?;
    }
    let loss = if label == 1.0 {
        -probability.max(f64::MIN_POSITIVE).ln()
    } else {
        -(1.0 - probability).max(f64::MIN_POSITIVE).ln()
    };
    Ok(val(Ty::Float, Data::Float(loss)))
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_sampled_softmax_update(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 8 {
        return Err(Error::new(
            line,
            "ML.sampled_softmax_update expects 8 arguments",
        ));
    }
    let values = evaluated_args(args, e)?;
    let output = float_array(&values[0], line)?;
    let output_bias = float_array(&values[1], line)?;
    let hidden = float_array(&values[2], line)?;
    let hidden_gradient = float_array(&values[3], line)?;
    let candidates = int_array(&values[4], line)?;
    let count = int_value(&values[5], line, "sample count")?;
    let rate = float_value(&values[6], line, "learning rate")?;
    let width = int_value(&values[7], line, "model width")?;
    if count == 0 || count > candidates.borrow().len() {
        return Err(Error::new(
            line,
            "sample count must fit the candidate array and be positive",
        ));
    }

    let hidden = hidden.borrow();
    let candidates = candidates.borrow();
    let mut output = output.borrow_mut();
    let mut output_bias = output_bias.borrow_mut();
    let mut hidden_gradient = hidden_gradient.borrow_mut();
    let mut candidate_ids = Vec::with_capacity(count);
    let mut scores = Vec::with_capacity(count);
    for sample in 0..count {
        let Data::Int(candidate) = &candidates[sample].data else {
            return Err(Error::new(line, "sample candidates must be int values"));
        };
        if *candidate < 0 {
            return Err(Error::new(line, "sample candidates cannot be negative"));
        }
        let candidate = *candidate as usize;
        if candidate >= output_bias.len() || candidate * width + width > output.len() {
            return Err(Error::new(
                line,
                "sample candidate is outside the vocabulary",
            ));
        }
        if candidate_ids.contains(&candidate) {
            return Err(Error::new(line, "sample candidates must be unique"));
        }
        let mut score = array_number(&output_bias, candidate, line)?;
        for unit in 0..width {
            score += array_number(&output, candidate * width + unit, line)?
                * array_number(&hidden, unit, line)?;
        }
        candidate_ids.push(candidate);
        scores.push(score);
    }

    let maximum = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let denominator: f64 = scores.iter().map(|score| (score - maximum).exp()).sum();
    let probabilities: Vec<f64> = scores
        .iter()
        .map(|score| (score - maximum).exp() / denominator)
        .collect();
    let loss = -probabilities[0].max(f64::MIN_POSITIVE).ln();

    for (sample, candidate) in candidate_ids.into_iter().enumerate() {
        let delta = probabilities[sample] - if sample == 0 { 1.0 } else { 0.0 };
        let bias = array_number(&output_bias, candidate, line)? - rate * delta;
        set_array_number(&mut output_bias, candidate, bias, line)?;
        for unit in 0..width {
            let index = candidate * width + unit;
            let weight = array_number(&output, index, line)?;
            let gradient = array_number(&hidden_gradient, unit, line)? + delta * weight;
            set_array_number(&mut hidden_gradient, unit, gradient, line)?;
            let updated = weight - rate * delta * array_number(&hidden, unit, line)?;
            set_array_number(&mut output, index, updated, line)?;
        }
    }
    Ok(val(Ty::Float, Data::Float(loss)))
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_mlp_backprop(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 14 {
        return Err(Error::new(line, "ML.mlp_backprop expects 14 arguments"));
    }
    let values = evaluated_args(args, e)?;
    let left_embed = float_array(&values[0], line)?;
    let left_matrix = float_array(&values[1], line)?;
    let middle_embed = float_array(&values[2], line)?;
    let middle_matrix = float_array(&values[3], line)?;
    let right_embed = float_array(&values[4], line)?;
    let right_matrix = float_array(&values[5], line)?;
    let bias = float_array(&values[6], line)?;
    let hidden = float_array(&values[7], line)?;
    let hidden_gradient = float_array(&values[8], line)?;
    let left = int_value(&values[9], line, "left word id")?;
    let middle = int_value(&values[10], line, "middle word id")?;
    let right = int_value(&values[11], line, "right word id")?;
    let width = int_value(&values[12], line, "MLP width")?;
    let rate = float_value(&values[13], line, "learning rate")?;

    let hidden = hidden.borrow();
    let hidden_gradient = hidden_gradient.borrow();
    let mut left_embed = left_embed.borrow_mut();
    let mut middle_embed = middle_embed.borrow_mut();
    let mut right_embed = right_embed.borrow_mut();
    let mut left_matrix = left_matrix.borrow_mut();
    let mut middle_matrix = middle_matrix.borrow_mut();
    let mut right_matrix = right_matrix.borrow_mut();
    let mut bias = bias.borrow_mut();
    let mut left_embed_gradient = vec![0.0; width];
    let mut middle_embed_gradient = vec![0.0; width];
    let mut right_embed_gradient = vec![0.0; width];

    for unit in 0..width {
        let activation = array_number(&hidden, unit, line)?;
        let gradient =
            array_number(&hidden_gradient, unit, line)? * (1.0 - activation * activation);
        let updated_bias = array_number(&bias, unit, line)? - rate * gradient;
        set_array_number(&mut bias, unit, updated_bias, line)?;
        for feature in 0..width {
            let matrix_index = unit * width + feature;
            let left_weight = array_number(&left_matrix, matrix_index, line)?;
            let middle_weight = array_number(&middle_matrix, matrix_index, line)?;
            let right_weight = array_number(&right_matrix, matrix_index, line)?;
            left_embed_gradient[feature] += gradient * left_weight;
            middle_embed_gradient[feature] += gradient * middle_weight;
            right_embed_gradient[feature] += gradient * right_weight;
            let left_input = array_number(&left_embed, left * width + feature, line)?;
            let middle_input = array_number(&middle_embed, middle * width + feature, line)?;
            let right_input = array_number(&right_embed, right * width + feature, line)?;
            set_array_number(
                &mut left_matrix,
                matrix_index,
                left_weight - rate * gradient * left_input,
                line,
            )?;
            set_array_number(
                &mut middle_matrix,
                matrix_index,
                middle_weight - rate * gradient * middle_input,
                line,
            )?;
            set_array_number(
                &mut right_matrix,
                matrix_index,
                right_weight - rate * gradient * right_input,
                line,
            )?;
        }
    }
    for feature in 0..width {
        let left_index = left * width + feature;
        let middle_index = middle * width + feature;
        let right_index = right * width + feature;
        let left_value =
            array_number(&left_embed, left_index, line)? - rate * left_embed_gradient[feature];
        let middle_value = array_number(&middle_embed, middle_index, line)?
            - rate * middle_embed_gradient[feature];
        let right_value =
            array_number(&right_embed, right_index, line)? - rate * right_embed_gradient[feature];
        set_array_number(&mut left_embed, left_index, left_value, line)?;
        set_array_number(&mut middle_embed, middle_index, middle_value, line)?;
        set_array_number(&mut right_embed, right_index, right_value, line)?;
    }
    Ok(val(Ty::Unit, Data::Unit))
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn random_unit() -> f64 {
    crate::native::next_random_u64() as f64 / u64::MAX as f64
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_softmax_sample(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 8 {
        return Err(Error::new(line, "ML.softmax_sample expects 8 arguments"));
    }
    let values = evaluated_args(args, e)?;
    let output = float_array(&values[0], line)?;
    let bias = float_array(&values[1], line)?;
    let hidden = float_array(&values[2], line)?;
    let vocabulary = int_value(&values[3], line, "vocabulary")?;
    let width = int_value(&values[4], line, "MLP width")?;
    let temperature = float_value(&values[5], line, "temperature")?;
    let excluded = int_value(&values[6], line, "excluded word id")?;
    let repetition_penalty = float_value(&values[7], line, "repetition penalty")?;
    if vocabulary <= 1 || temperature <= 0.0 {
        return Err(Error::new(
            line,
            "softmax sampling needs vocabulary > 1 and temperature > 0",
        ));
    }
    let output = output.borrow();
    let bias = bias.borrow();
    let hidden = hidden.borrow();
    let mut logits = Vec::with_capacity(vocabulary - 1);
    let mut maximum = f64::NEG_INFINITY;
    for candidate in 1..vocabulary {
        let mut score = array_number(&bias, candidate, line)?;
        for unit in 0..width {
            score += array_number(&output, candidate * width + unit, line)?
                * array_number(&hidden, unit, line)?;
        }
        if candidate == excluded {
            score -= repetition_penalty;
        }
        score /= temperature;
        maximum = maximum.max(score);
        logits.push(score);
    }
    let total: f64 = logits.iter().map(|score| (score - maximum).exp()).sum();
    let draw = random_unit() * total;
    let mut cumulative = 0.0;
    for (offset, score) in logits.into_iter().enumerate() {
        cumulative += (score - maximum).exp();
        if draw <= cumulative {
            return Ok(val(Ty::Int, Data::Int((offset + 1) as i64)));
        }
    }
    Ok(val(Ty::Int, Data::Int((vocabulary - 1) as i64)))
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn logistic(number: f64) -> f64 {
    if number >= 0.0 {
        1.0 / (1.0 + (-number).exp())
    } else {
        let exp = number.exp();
        exp / (1.0 + exp)
    }
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_gru_forward(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 17 {
        return Err(Error::new(line, "ML.gru_forward expects 17 arguments"));
    }
    let values = evaluated_args(args, e)?;
    let state = float_array(&values[0], line)?;
    let previous = float_array(&values[1], line)?;
    let update = float_array(&values[2], line)?;
    let reset = float_array(&values[3], line)?;
    let candidate = float_array(&values[4], line)?;
    let embedding = float_array(&values[5], line)?;
    let token = int_value(&values[6], line, "token id")?;
    let update_input = float_array(&values[7], line)?;
    let update_state = float_array(&values[8], line)?;
    let update_bias = float_array(&values[9], line)?;
    let reset_input = float_array(&values[10], line)?;
    let reset_state = float_array(&values[11], line)?;
    let reset_bias = float_array(&values[12], line)?;
    let candidate_input = float_array(&values[13], line)?;
    let candidate_state = float_array(&values[14], line)?;
    let candidate_bias = float_array(&values[15], line)?;
    let width = int_value(&values[16], line, "GRU width")?;

    let embedding = embedding.borrow();
    let update_input = update_input.borrow();
    let update_state = update_state.borrow();
    let update_bias = update_bias.borrow();
    let reset_input = reset_input.borrow();
    let reset_state = reset_state.borrow();
    let reset_bias = reset_bias.borrow();
    let candidate_input = candidate_input.borrow();
    let candidate_state = candidate_state.borrow();
    let candidate_bias = candidate_bias.borrow();
    let mut state = state.borrow_mut();
    let mut previous = previous.borrow_mut();
    let mut update = update.borrow_mut();
    let mut reset = reset.borrow_mut();
    let mut candidate = candidate.borrow_mut();

    for unit in 0..width {
        let old = array_number(&state, unit, line)?;
        set_array_number(&mut previous, unit, old, line)?;
    }
    for unit in 0..width {
        let mut update_sum = array_number(&update_bias, unit, line)?;
        let mut reset_sum = array_number(&reset_bias, unit, line)?;
        for feature in 0..width {
            let input = array_number(&embedding, token * width + feature, line)?;
            let old = array_number(&previous, feature, line)?;
            update_sum += array_number(&update_input, unit * width + feature, line)? * input
                + array_number(&update_state, unit * width + feature, line)? * old;
            reset_sum += array_number(&reset_input, unit * width + feature, line)? * input
                + array_number(&reset_state, unit * width + feature, line)? * old;
        }
        set_array_number(&mut update, unit, logistic(update_sum), line)?;
        set_array_number(&mut reset, unit, logistic(reset_sum), line)?;
    }
    for unit in 0..width {
        let mut sum = array_number(&candidate_bias, unit, line)?;
        for feature in 0..width {
            let input = array_number(&embedding, token * width + feature, line)?;
            let gated_old =
                array_number(&reset, feature, line)? * array_number(&previous, feature, line)?;
            sum += array_number(&candidate_input, unit * width + feature, line)? * input
                + array_number(&candidate_state, unit * width + feature, line)? * gated_old;
        }
        let proposed = sum.tanh();
        let gate = array_number(&update, unit, line)?;
        let old = array_number(&previous, unit, line)?;
        set_array_number(&mut candidate, unit, proposed, line)?;
        set_array_number(&mut state, unit, (1.0 - gate) * old + gate * proposed, line)?;
    }
    Ok(val(Ty::Unit, Data::Unit))
}

#[cfg(feature = "ml-kernels")]
pub(crate) fn array_gru_backprop(args: &[Expr], e: EnvRef, line: usize) -> Result<Value> {
    if args.len() != 19 {
        return Err(Error::new(line, "ML.gru_backprop expects 19 arguments"));
    }
    let values = evaluated_args(args, e)?;
    let embedding = float_array(&values[0], line)?;
    let token = int_value(&values[1], line, "token id")?;
    let previous = float_array(&values[2], line)?;
    let update = float_array(&values[3], line)?;
    let reset = float_array(&values[4], line)?;
    let candidate = float_array(&values[5], line)?;
    let hidden_gradient = float_array(&values[6], line)?;
    let recurrent_gradient = float_array(&values[7], line)?;
    let update_input = float_array(&values[8], line)?;
    let update_state = float_array(&values[9], line)?;
    let update_bias = float_array(&values[10], line)?;
    let reset_input = float_array(&values[11], line)?;
    let reset_state = float_array(&values[12], line)?;
    let reset_bias = float_array(&values[13], line)?;
    let candidate_input = float_array(&values[14], line)?;
    let candidate_state = float_array(&values[15], line)?;
    let candidate_bias = float_array(&values[16], line)?;
    let width = int_value(&values[17], line, "GRU width")?;
    let rate = float_value(&values[18], line, "learning rate")?;

    let previous = previous.borrow();
    let update = update.borrow();
    let reset = reset.borrow();
    let candidate = candidate.borrow();
    let hidden_gradient = hidden_gradient.borrow();
    let recurrent_input = recurrent_gradient.borrow();
    let mut embedding = embedding.borrow_mut();
    let mut update_input = update_input.borrow_mut();
    let mut update_state = update_state.borrow_mut();
    let mut update_bias = update_bias.borrow_mut();
    let mut reset_input = reset_input.borrow_mut();
    let mut reset_state = reset_state.borrow_mut();
    let mut reset_bias = reset_bias.borrow_mut();
    let mut candidate_input = candidate_input.borrow_mut();
    let mut candidate_state = candidate_state.borrow_mut();
    let mut candidate_bias = candidate_bias.borrow_mut();

    let mut update_pre = vec![0.0; width];
    let mut reset_pre = vec![0.0; width];
    let mut candidate_pre = vec![0.0; width];
    let mut total_gradient = vec![0.0; width];
    for unit in 0..width {
        let gradient = array_number(&hidden_gradient, unit, line)?
            + array_number(&recurrent_input, unit, line)?;
        total_gradient[unit] = gradient;
        let gate = array_number(&update, unit, line)?;
        let proposed = array_number(&candidate, unit, line)?;
        let old = array_number(&previous, unit, line)?;
        candidate_pre[unit] = gradient * gate * (1.0 - proposed * proposed);
        update_pre[unit] = gradient * (proposed - old) * gate * (1.0 - gate);
    }
    for (feature, reset_slot) in reset_pre.iter_mut().enumerate() {
        let mut reset_gradient = 0.0;
        for (unit, candidate_gradient) in candidate_pre.iter().enumerate() {
            reset_gradient += candidate_gradient
                * array_number(&candidate_state, unit * width + feature, line)?
                * array_number(&previous, feature, line)?;
        }
        let gate = array_number(&reset, feature, line)?;
        *reset_slot = reset_gradient * gate * (1.0 - gate);
    }

    // Carry dL/d(previous state) into the preceding item in the BPTT window.
    // This must be computed before any recurrent matrix is updated below.
    let mut previous_gradient = vec![0.0; width];
    for (feature, previous_slot) in previous_gradient.iter_mut().enumerate() {
        let gate = array_number(&update, feature, line)?;
        let reset_value = array_number(&reset, feature, line)?;
        let mut gradient = total_gradient[feature] * (1.0 - gate);
        for unit in 0..width {
            let index = unit * width + feature;
            gradient += update_pre[unit] * array_number(&update_state, index, line)?;
            gradient += reset_pre[unit] * array_number(&reset_state, index, line)?;
            gradient +=
                candidate_pre[unit] * array_number(&candidate_state, index, line)? * reset_value;
        }
        *previous_slot = gradient;
    }
    drop(recurrent_input);
    {
        let mut recurrent_output = recurrent_gradient.borrow_mut();
        for (feature, gradient) in previous_gradient.iter().enumerate() {
            set_array_number(&mut recurrent_output, feature, *gradient, line)?;
        }
    }

    let mut embedding_gradient = vec![0.0; width];
    for unit in 0..width {
        let input_gate_gradient = update_pre[unit];
        let reset_gate_gradient = reset_pre[unit];
        let proposal_gradient = candidate_pre[unit];
        let ub = array_number(&update_bias, unit, line)? - rate * input_gate_gradient;
        let rb = array_number(&reset_bias, unit, line)? - rate * reset_gate_gradient;
        let cb = array_number(&candidate_bias, unit, line)? - rate * proposal_gradient;
        set_array_number(&mut update_bias, unit, ub, line)?;
        set_array_number(&mut reset_bias, unit, rb, line)?;
        set_array_number(&mut candidate_bias, unit, cb, line)?;
        for (feature, embedding_gradient_slot) in embedding_gradient.iter_mut().enumerate() {
            let index = unit * width + feature;
            let input = array_number(&embedding, token * width + feature, line)?;
            let old = array_number(&previous, feature, line)?;
            let gated_old = array_number(&reset, feature, line)? * old;
            let uw = array_number(&update_input, index, line)?;
            let rw = array_number(&reset_input, index, line)?;
            let cw = array_number(&candidate_input, index, line)?;
            *embedding_gradient_slot +=
                input_gate_gradient * uw + reset_gate_gradient * rw + proposal_gradient * cw;
            set_array_number(
                &mut update_input,
                index,
                uw - rate * input_gate_gradient * input,
                line,
            )?;
            set_array_number(
                &mut reset_input,
                index,
                rw - rate * reset_gate_gradient * input,
                line,
            )?;
            set_array_number(
                &mut candidate_input,
                index,
                cw - rate * proposal_gradient * input,
                line,
            )?;
            let uu = array_number(&update_state, index, line)?;
            let ru = array_number(&reset_state, index, line)?;
            let cu = array_number(&candidate_state, index, line)?;
            set_array_number(
                &mut update_state,
                index,
                uu - rate * input_gate_gradient * old,
                line,
            )?;
            set_array_number(
                &mut reset_state,
                index,
                ru - rate * reset_gate_gradient * old,
                line,
            )?;
            set_array_number(
                &mut candidate_state,
                index,
                cu - rate * proposal_gradient * gated_old,
                line,
            )?;
        }
    }
    for (feature, gradient) in embedding_gradient.iter().enumerate() {
        let index = token * width + feature;
        let updated = array_number(&embedding, index, line)? - rate * gradient;
        set_array_number(&mut embedding, index, updated, line)?;
    }
    Ok(val(Ty::Unit, Data::Unit))
}
