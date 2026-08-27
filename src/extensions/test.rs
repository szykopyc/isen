use crate::native::{NativeCall, NativeExpected as Expected, NativeFunction as Function, NativeProduced as Produced, NativeRegistry, NativeSignature as Signature, NativeSpace as Space};
use crate::{values_equal, Result, Ty, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(Space {
        name: "Test",
        functions: &[
            Function {
                name: "assert",
                call: assert,
            },
            Function {
                name: "equal",
                call: equal,
            },
            Function {
                name: "not_equal",
                call: not_equal,
            },
            Function {
                name: "fail",
                call: fail,
            },
        ],
        signatures: || vec![
            Signature::exact("assert", vec![Ty::Bool, Ty::String], Ty::Unit),
            Signature::custom("equal", vec![Expected::Any, Expected::SameAs(0), Expected::Exact(Ty::String)], Produced::Exact(Ty::Unit)),
            Signature::custom("not_equal", vec![Expected::Any, Expected::SameAs(0), Expected::Exact(Ty::String)], Produced::Exact(Ty::Unit)),
            Signature::exact("fail", vec![Ty::String], Ty::Unit),
        ],
    });
}

fn assert(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(2, "Test.assert")?;
    if !call.bool(0, "Test.assert")? {
        return Err(call.error(call.string(1, "Test.assert")?));
    }
    Ok(call.unit_value())
}

fn equal(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(3, "Test.equal")?;
    if !values_equal(call.value(0, "Test.equal")?, call.value(1, "Test.equal")?) {
        return Err(call.error(format!(
            "{}: expected {}, got {}",
            call.string(2, "Test.equal")?,
            call.shown(1, "Test.equal")?,
            call.shown(0, "Test.equal")?,
        )));
    }
    Ok(call.unit_value())
}

fn not_equal(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(3, "Test.not_equal")?;
    if values_equal(
        call.value(0, "Test.not_equal")?,
        call.value(1, "Test.not_equal")?,
    ) {
        return Err(call.error(format!(
            "{}: values were both {}",
            call.string(2, "Test.not_equal")?,
            call.shown(0, "Test.not_equal")?,
        )));
    }
    Ok(call.unit_value())
}

fn fail(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Test.fail")?;
    Err(call.error(call.string(0, "Test.fail")?))
}
