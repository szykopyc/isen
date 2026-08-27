use std::{
    sync::OnceLock,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::native::{NativeCall, NativeFunction, NativeRegistry, NativeSignature as Signature, NativeSpace};
use crate::{Result, Ty};

static START: OnceLock<Instant> = OnceLock::new();

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add(NativeSpace {
        name: "Time",
        functions: &[
            NativeFunction {
                name: "clock",
                call: clock,
            },
            NativeFunction {
                name: "sleep",
                call: sleep,
            },
            NativeFunction {
                name: "since",
                call: since,
            },
            NativeFunction {
                name: "unix_millis",
                call: unix_millis,
            },
            NativeFunction {
                name: "utc",
                call: utc,
            },
        ],
        signatures: || vec![
            Signature::exact("clock", vec![], Ty::Int),
            Signature::exact("sleep", vec![Ty::Int], Ty::Unit),
            Signature::exact("since", vec![Ty::Int], Ty::Int),
            Signature::exact("unix_millis", vec![], Ty::Int),
            Signature::exact("utc", vec![], Ty::String),
        ],
    });
}

fn milliseconds() -> i64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

fn clock(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "Time.clock")?;
    Ok(call.int_value(milliseconds()))
}

fn sleep(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(1, "Time.sleep")?;
    let duration = call.int(0, "Time.sleep")?;
    if duration < 0 {
        return Err(call.error("Time.sleep cannot sleep a negative duration"));
    }
    thread::sleep(Duration::from_millis(duration as u64));
    Ok(call.unit_value())
}

fn since(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(1, "Time.since")?;
    Ok(call.int_value(milliseconds() - call.int(0, "Time.since")?))
}

fn wall_time(call: &NativeCall<'_>, signature: &str) -> Result<Duration> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| call.error(format!("{signature} failed: {error}")))
}

fn unix_millis(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "Time.unix_millis")?;
    let milliseconds = wall_time(&call, "Time.unix_millis")?.as_millis();
    let milliseconds = i64::try_from(milliseconds)
        .map_err(|_| call.error("Time.unix_millis is outside Isen's integer range"))?;
    Ok(call.int_value(milliseconds))
}

fn utc(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "Time.utc")?;
    let time = wall_time(&call, "Time.utc")?;
    let seconds = time.as_secs();
    let days = i64::try_from(seconds / 86_400)
        .map_err(|_| call.error("Time.utc is outside its supported range"))?;
    let seconds_in_day = seconds % 86_400;
    let (year, month, day) = civil_date(days);
    let hour = seconds_in_day / 3_600;
    let minute = seconds_in_day % 3_600 / 60;
    let second = seconds_in_day % 60;
    Ok(call.string_value(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        time.subsec_millis()
    )))
}

// Gregorian civil date from days since 1970-01-01. This keeps Time independent
// of a heavyweight date dependency while remaining deterministic on all hosts.
fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096)
            / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = month_position + if month_position < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::civil_date;

    #[test]
    fn converts_epoch_days_to_gregorian_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
        assert_eq!(civil_date(20_453), (2025, 12, 31));
    }
}
