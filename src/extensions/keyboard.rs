use std::{cell::RefCell, io, mem};

use crate::native::{
    NativeCall, NativeFunction as Function, NativeRegistry, NativeSignature as Signature,
    NativeSpace as Space,
};
use crate::{Data, Result, Ty, Value, val};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add_cleanup(shutdown);
    registry.add(Space {
        name: "Keyboard",
        functions: &[
            Function { name: "open", call: open },
            Function { name: "read", call: read },
            Function { name: "key", call: key },
            Function { name: "wait", call: wait },
            Function { name: "close", call: close },
            Function { name: "active", call: active },
        ],
        signatures: || {
            let key = Ty::Perchance(Box::new(Ty::String));
            vec![
                Signature::exact("open", vec![], Ty::Unit),
                Signature::exact("read", vec![], key.clone()),
                Signature::exact("key", vec![], Ty::String),
                Signature::exact("wait", vec![Ty::Int], key),
                Signature::exact("close", vec![], Ty::Unit),
                Signature::exact("active", vec![], Ty::Bool),
            ]
        },
    });
}

fn shutdown() {
    #[cfg(unix)]
    STATE.with(|state| {
        let _ = state.borrow_mut().restore();
    });
}

#[cfg(unix)]
#[derive(Default)]
struct State {
    original_terminal: Option<libc::termios>,
    original_flags: libc::c_int,
    pending: Vec<u8>,
}

#[cfg(unix)]
impl State {
    fn is_active(&self) -> bool {
        self.original_terminal.is_some()
    }

    fn restore(&mut self) -> io::Result<()> {
        let Some(original) = self.original_terminal.take() else {
            return Ok(());
        };
        let flags_result = unsafe {
            if libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, self.original_flags) == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        };
        let terminal_result = unsafe {
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &original) == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        };
        self.pending.clear();
        flags_result.and(terminal_result)
    }
}

#[cfg(unix)]
impl Drop for State {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(unix)]
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn optional_key(key: Option<String>) -> Value {
    match key {
        Some(key) => val(
            Ty::Perchance(Box::new(Ty::String)),
            Data::String(key),
        ),
        None => val(Ty::Perchance(Box::new(Ty::String)), Data::Naught),
    }
}

#[cfg(unix)]
fn open(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Keyboard.open")?;
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.is_active() {
            return Ok(call.unit_value());
        }
        if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
            return Err(call.error("Keyboard.open requires an interactive terminal on stdin"));
        }

        let mut original = unsafe { mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } == -1 {
            return Err(call.error(format!(
                "Keyboard.open failed: {}",
                io::Error::last_os_error()
            )));
        }
        let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
        if flags == -1 {
            return Err(call.error(format!(
                "Keyboard.open failed: {}",
                io::Error::last_os_error()
            )));
        }

        let mut raw = original;
        unsafe { libc::cfmakeraw(&mut raw) };
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } == -1 {
            return Err(call.error(format!(
                "Keyboard.open failed: {}",
                io::Error::last_os_error()
            )));
        }
        if unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_SETFL, flags | libc::O_NONBLOCK) }
            == -1
        {
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &original) };
            return Err(call.error(format!(
                "Keyboard.open failed: {}",
                io::Error::last_os_error()
            )));
        }

        state.original_terminal = Some(original);
        state.original_flags = flags;
        state.pending.clear();
        Ok(call.unit_value())
    })
}

#[cfg(not(unix))]
fn open(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Keyboard.open")?;
    Err(call.error("Keyboard is currently supported on Unix terminals"))
}

#[cfg(unix)]
fn fill_pending(state: &mut State) -> io::Result<()> {
    loop {
        let mut bytes = [0u8; 64];
        let count = unsafe {
            libc::read(
                libc::STDIN_FILENO,
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if count > 0 {
            state.pending.extend_from_slice(&bytes[..count as usize]);
            continue;
        }
        if count == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::WouldBlock {
            return Ok(());
        }
        return Err(error);
    }
}

#[cfg(unix)]
fn take_key(bytes: &mut Vec<u8>) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let named = [
        (b"\x1b[A".as_slice(), "up"),
        (b"\x1b[B".as_slice(), "down"),
        (b"\x1b[C".as_slice(), "right"),
        (b"\x1b[D".as_slice(), "left"),
        (b"\x1b[H".as_slice(), "home"),
        (b"\x1b[F".as_slice(), "end"),
        (b"\x1b[3~".as_slice(), "delete"),
        (b"\x1b[5~".as_slice(), "page_up"),
        (b"\x1b[6~".as_slice(), "page_down"),
    ];
    for (sequence, name) in named {
        if bytes.starts_with(sequence) {
            bytes.drain(..sequence.len());
            return Some(name.into());
        }
        if bytes.len() > 1 && sequence.starts_with(bytes.as_slice()) {
            return None;
        }
    }
    if bytes[0] == 0x1b {
        bytes.remove(0);
        return Some("escape".into());
    }

    let (name, length) = match bytes[0] {
        b'\r' | b'\n' => (Some("enter".into()), 1),
        b'\t' => (Some("tab".into()), 1),
        0x7f | 0x08 => (Some("backspace".into()), 1),
        0x01..=0x1a => (Some(format!("ctrl_{}", (b'a' + bytes[0] - 1) as char)), 1),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => {
                let character = text.chars().next().unwrap();
                (Some(character.to_string()), character.len_utf8())
            }
            Err(error) if error.error_len().is_none() => return None,
            Err(_) => (Some("replacement".into()), 1),
        },
    };
    bytes.drain(..length);
    name
}

#[cfg(unix)]
fn read_key(call: &NativeCall<'_>, timeout_milliseconds: Option<libc::c_int>) -> Result<Value> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if !state.is_active() {
            return Err(call.error("call Keyboard.open before reading keys"));
        }
        if let Some(key) = take_key(&mut state.pending) {
            return Ok(optional_key(Some(key)));
        }
        if let Some(milliseconds) = timeout_milliseconds {
            let mut descriptor = libc::pollfd {
                fd: libc::STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut descriptor, 1, milliseconds) };
            if result == -1 {
                return Err(call.error(format!(
                    "Keyboard.wait failed: {}",
                    io::Error::last_os_error()
                )));
            }
            if result == 0 {
                return Ok(optional_key(None));
            }
        }
        fill_pending(&mut state).map_err(|error| call.error(format!("Keyboard.read failed: {error}")))?;
        Ok(optional_key(take_key(&mut state.pending)))
    })
}

fn read(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Keyboard.read")?;
    #[cfg(unix)]
    {
        read_key(&call, None)
    }
    #[cfg(not(unix))]
    {
        Err(call.error("Keyboard is currently supported on Unix terminals"))
    }
}

fn key(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Keyboard.key")?;
    #[cfg(unix)]
    loop {
        let value = read_key(&call, Some(-1))?;
        if let Data::String(key) = value.data {
            return Ok(val(Ty::String, Data::String(key)));
        }
    }
    #[cfg(not(unix))]
    {
        Err(call.error("Keyboard is currently supported on Unix terminals"))
    }
}

fn wait(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(1, "Keyboard.wait")?;
    let milliseconds = call.int(0, "Keyboard.wait")?;
    if milliseconds < 0 {
        return Err(call.error("Keyboard.wait timeout cannot be negative"));
    }
    #[cfg(unix)]
    {
        let milliseconds = milliseconds.min(libc::c_int::MAX as i64) as libc::c_int;
        read_key(&call, Some(milliseconds))
    }
    #[cfg(not(unix))]
    {
        Err(call.error("Keyboard is currently supported on Unix terminals"))
    }
}

fn close(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Keyboard.close")?;
    #[cfg(unix)]
    {
        STATE.with(|state| {
            state
                .borrow_mut()
                .restore()
                .map_err(|error| call.error(format!("Keyboard.close failed: {error}")))?;
            Ok(call.unit_value())
        })
    }
    #[cfg(not(unix))]
    {
        Ok(call.unit_value())
    }
}

fn active(call: NativeCall<'_>) -> Result<Value> {
    call.exactly(0, "Keyboard.active")?;
    #[cfg(unix)]
    {
        Ok(call.bool_value(STATE.with(|state| state.borrow().is_active())))
    }
    #[cfg(not(unix))]
    {
        Ok(call.bool_value(false))
    }
}

#[cfg(test)]
mod tests {
    use super::take_key;

    #[test]
    fn decodes_named_and_utf8_keys_without_losing_following_input() {
        let mut bytes = b"\x1b[Aq".to_vec();
        assert_eq!(take_key(&mut bytes).as_deref(), Some("up"));
        assert_eq!(take_key(&mut bytes).as_deref(), Some("q"));

        let mut bytes = "é".as_bytes().to_vec();
        assert_eq!(take_key(&mut bytes).as_deref(), Some("é"));
        assert!(bytes.is_empty());
    }
}
