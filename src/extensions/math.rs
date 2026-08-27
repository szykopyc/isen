use std::f64::consts;

use crate::native::{NativeCall, NativeConstant, NativeExpected as Expected, NativeFunction, NativeProduced as Produced, NativeRegistry, NativeSignature as Signature, NativeSpace};
use crate::{val, Data, Result, Ty};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(NativeSpace {
        name: "Maths",
        functions: &[
            NativeFunction {
                name: "exp",
                call: exp,
            },
            NativeFunction {
                name: "log",
                call: log,
            },
            NativeFunction {
                name: "tanh",
                call: tanh,
            },
            NativeFunction {
                name: "sqrt",
                call: sqrt,
            },
            NativeFunction {
                name: "sin",
                call: sin,
            },
            NativeFunction {
                name: "cos",
                call: cos,
            },
            NativeFunction {
                name: "abs",
                call: abs,
            },
            NativeFunction {
                name: "floor",
                call: floor,
            },
            NativeFunction {
                name: "min",
                call: min,
            },
            NativeFunction {
                name: "max",
                call: max,
            },
            NativeFunction {
                name: "pow",
                call: pow,
            },
        ],
        signatures: || {
            let mut signatures = ["exp", "log", "tanh", "sqrt", "sin", "cos"]
                .into_iter()
                .map(|name| Signature::exact(name, vec![Ty::Float], Ty::Float))
                .collect::<Vec<_>>();
            signatures.push(Signature::custom("abs", vec![Expected::Number], Produced::SameAs(0)));
            signatures.push(Signature::exact("floor", vec![Ty::Float], Ty::Int));
            signatures.push(Signature::custom("min", vec![Expected::Number, Expected::SameAs(0)], Produced::SameAs(0)));
            signatures.push(Signature::custom("max", vec![Expected::Number, Expected::SameAs(0)], Produced::SameAs(0)));
            signatures.push(Signature::exact("pow", vec![Ty::Float, Ty::Float], Ty::Float));
            signatures
        },
    });
    for (name, value) in [
        ("pi", consts::PI),
        ("tau", consts::TAU),
        ("e", consts::E),
        ("phi", 1.618_033_988_749_895),
        ("sqrt_two", consts::SQRT_2),
        ("ln_two", consts::LN_2),
    ] {
        registry.add_constant("Maths", name, NativeConstant::Float(value));
    }
}

fn unary(call: NativeCall<'_>, name: &str, operation: fn(f64) -> f64) -> Result<crate::Value> {
    let signature = format!("Maths.{name}");
    call.exactly(1, &signature)?;
    let input = call.float(0, &signature)?;
    if name == "log" && input <= 0.0 {
        return Err(call.error("Maths.log expects a positive float"));
    }
    if name == "sqrt" && input < 0.0 {
        return Err(call.error("Maths.sqrt expects a non-negative float"));
    }
    Ok(call.float_value(operation(input)))
}

fn exp(call: NativeCall<'_>) -> Result<crate::Value> {
    unary(call, "exp", f64::exp)
}
fn log(call: NativeCall<'_>) -> Result<crate::Value> {
    unary(call, "log", f64::ln)
}
fn tanh(call: NativeCall<'_>) -> Result<crate::Value> {
    unary(call, "tanh", f64::tanh)
}
fn sqrt(call: NativeCall<'_>) -> Result<crate::Value> {
    unary(call, "sqrt", f64::sqrt)
}
fn sin(call: NativeCall<'_>) -> Result<crate::Value> {
    unary(call, "sin", f64::sin)
}
fn cos(call: NativeCall<'_>) -> Result<crate::Value> {
    unary(call, "cos", f64::cos)
}

fn abs(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(1, "Maths.abs")?;
    match &call.value(0, "Maths.abs")?.data {
        Data::Int(value) => value
            .checked_abs()
            .map(|value| val(Ty::Int, Data::Int(value)))
            .ok_or_else(|| call.error("Maths.abs cannot represent abs of the smallest int")),
        Data::Float(value) => Ok(call.float_value(value.abs())),
        _ => Err(call.error("Maths.abs expects int or float")),
    }
}

fn floor(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(1, "Maths.floor")?;
    let value = call.float(0, "Maths.floor")?.floor();
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err(call.error("Maths.floor result is outside the int range"));
    }
    Ok(call.int_value(value as i64))
}

fn min(call: NativeCall<'_>) -> Result<crate::Value> {
    extremum(call, "min", false)
}

fn max(call: NativeCall<'_>) -> Result<crate::Value> {
    extremum(call, "max", true)
}

fn extremum(call: NativeCall<'_>, name: &str, greatest: bool) -> Result<crate::Value> {
    let signature = format!("Maths.{name}");
    call.exactly(2, &signature)?;
    match (
        &call.value(0, &signature)?.data,
        &call.value(1, &signature)?.data,
    ) {
        (Data::Int(left), Data::Int(right)) => Ok(call.int_value(if greatest {
            (*left).max(*right)
        } else {
            (*left).min(*right)
        })),
        (Data::Float(left), Data::Float(right)) => Ok(call.float_value(if greatest {
            left.max(*right)
        } else {
            left.min(*right)
        })),
        _ => Err(call.error(format!(
            "{signature} expects two values of the same numeric type"
        ))),
    }
}

fn pow(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(2, "Maths.pow")?;
    Ok(call.float_value(
        call.float(0, "Maths.pow")?
            .powf(call.float(1, "Maths.pow")?),
    ))
}
