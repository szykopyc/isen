use std::{
    cell::{Cell, RefCell},
    io::{self, Write},
    rc::Rc,
};

use crate::native::{NativeCall, NativeExpected as Expected, NativeFunction, NativeProduced as Produced, NativeRegistry, NativeSignature as Signature, NativeSpace};
use crate::{Data, Result, Ty, val};

thread_local! {
    static SCREEN_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.add_cleanup(shutdown);
    registry.add(NativeSpace {
        name: "LengText",
        functions: &[
            // Original colours.
            NativeFunction {
                name: "red",
                call: red,
            },
            NativeFunction {
                name: "yellow",
                call: yellow,
            },
            NativeFunction {
                name: "green",
                call: green,
            },
            NativeFunction {
                name: "blue",
                call: blue,
            },
            // Standard ANSI colours.
            NativeFunction {
                name: "black",
                call: black,
            },
            NativeFunction {
                name: "magenta",
                call: magenta,
            },
            NativeFunction {
                name: "cyan",
                call: cyan,
            },
            NativeFunction {
                name: "white",
                call: white,
            },
            NativeFunction {
                name: "grey",
                call: grey,
            },
            // Bright ANSI colours.
            NativeFunction {
                name: "bright_red",
                call: bright_red,
            },
            NativeFunction {
                name: "bright_yellow",
                call: bright_yellow,
            },
            NativeFunction {
                name: "bright_green",
                call: bright_green,
            },
            NativeFunction {
                name: "bright_blue",
                call: bright_blue,
            },
            NativeFunction {
                name: "bright_magenta",
                call: bright_magenta,
            },
            NativeFunction {
                name: "bright_cyan",
                call: bright_cyan,
            },
            NativeFunction {
                name: "bright_white",
                call: bright_white,
            },
            // Useful 256-colour additions.
            NativeFunction {
                name: "orange",
                call: orange,
            },
            NativeFunction {
                name: "pink",
                call: pink,
            },
            NativeFunction {
                name: "purple",
                call: purple,
            },
            NativeFunction {
                name: "violet",
                call: violet,
            },
            NativeFunction {
                name: "teal",
                call: teal,
            },
            NativeFunction {
                name: "lime",
                call: lime,
            },
            NativeFunction {
                name: "gold",
                call: gold,
            },
            NativeFunction {
                name: "sky",
                call: sky,
            },
            NativeFunction {
                name: "flush",
                call: flush,
            },
            NativeFunction { name: "blit", call: blit },
            NativeFunction { name: "screen_begin", call: screen_begin },
            NativeFunction { name: "screen_end", call: screen_end },
            NativeFunction { name: "size", call: size },
            NativeFunction {
                name: "palette",
                call: palette,
            },
            NativeFunction {
                name: "indent",
                call: indent,
            },
            NativeFunction {
                name: "pretty_json",
                call: pretty_json,
            },
        ],
        signatures: || {
            let mut signatures = [
                "red", "yellow", "green", "blue", "black", "magenta", "cyan", "white",
                "grey", "bright_red", "bright_yellow", "bright_green", "bright_blue",
                "bright_magenta", "bright_cyan", "bright_white", "orange", "pink", "purple",
                "violet", "teal", "lime", "gold", "sky",
            ]
            .into_iter()
            .map(|name| Signature::custom(name, vec![Expected::Any], Produced::Exact(Ty::String)))
            .collect::<Vec<_>>();
            signatures.push(Signature::exact("flush", vec![], Ty::Unit));
            signatures.push(Signature::exact("blit", vec![Ty::String], Ty::Unit));
            signatures.push(Signature::exact("screen_begin", vec![], Ty::Unit));
            signatures.push(Signature::exact("screen_end", vec![], Ty::Unit));
            signatures.push(Signature::exact(
                "size",
                vec![],
                Ty::List(Box::new(Ty::Int)),
            ));
            signatures.push(Signature::exact("palette", vec![], Ty::Unit));
            signatures.push(Signature::exact("indent", vec![Ty::String, Ty::Int], Ty::String));
            signatures.push(Signature::exact("pretty_json", vec![Ty::Json, Ty::Int], Ty::String));
            signatures
        },
    });
}

fn indent(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(2, "LengText.indent")?;
    let value = call.string(0, "LengText.indent")?;
    let spaces = call.int(1, "LengText.indent")?;
    if !(0..=64).contains(&spaces) {
        return Err(call.error("LengText.indent expects between 0 and 64 spaces"));
    }
    let prefix = " ".repeat(spaces as usize);
    let output = value
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(call.string_value(output))
}

fn pretty_json(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(2, "LengText.pretty_json")?;
    let spaces = call.int(1, "LengText.pretty_json")?;
    let output = crate::extensions::extension_json::pretty_string(
        call.json(0, "LengText.pretty_json")?,
        spaces,
    )
    .map_err(|error| call.error(format!("LengText.pretty_json failed: {error}")))?;
    Ok(call.string_value(output))
}

fn colour(call: NativeCall<'_>, name: &str, ansi: &str) -> Result<crate::Value> {
    let signature = format!("LengText.{name}");
    call.exactly(1, &signature)?;

    Ok(call.string_value(format!("\x1b[{ansi}m{}\x1b[0m", call.shown(0, &signature)?)))
}

// ---------------------------------------------------------------------------
// Original colours
// ---------------------------------------------------------------------------

fn red(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "red", "31")
}

fn yellow(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "yellow", "33")
}

fn green(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "green", "32")
}

fn blue(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "blue", "34")
}

// ---------------------------------------------------------------------------
// Standard ANSI colours
// ---------------------------------------------------------------------------

fn black(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "black", "30")
}

fn magenta(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "magenta", "35")
}

fn cyan(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "cyan", "36")
}

fn white(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "white", "37")
}

fn grey(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "grey", "90")
}

// ---------------------------------------------------------------------------
// Bright ANSI colours
// ---------------------------------------------------------------------------

fn bright_red(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_red", "91")
}

fn bright_green(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_green", "92")
}

fn bright_yellow(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_yellow", "93")
}

fn bright_blue(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_blue", "94")
}

fn bright_magenta(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_magenta", "95")
}

fn bright_cyan(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_cyan", "96")
}

fn bright_white(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "bright_white", "97")
}

// ---------------------------------------------------------------------------
// 256-colour palette
// ---------------------------------------------------------------------------
//
// These use ANSI's:
//     38;5;<colour>
//
// They should work in basically any remotely modern terminal, including
// Ghostty, iTerm2, Kitty, WezTerm, most Linux terminals, etc.

fn orange(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "orange", "38;5;208")
}

fn pink(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "pink", "38;5;213")
}

fn purple(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "purple", "38;5;129")
}

fn violet(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "violet", "38;5;177")
}

fn teal(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "teal", "38;5;37")
}

fn lime(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "lime", "38;5;118")
}

fn gold(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "gold", "38;5;220")
}

fn sky(call: NativeCall<'_>) -> Result<crate::Value> {
    colour(call, "sky", "38;5;117")
}

// ---------------------------------------------------------------------------
// Terminal
// ---------------------------------------------------------------------------

fn flush(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "LengText.flush")?;

    print!("\x1b[2J\x1b[H");

    io::stdout()
        .flush()
        .map_err(|error| call.error(format!("LengText.flush failed: {error}")))?;

    Ok(call.unit_value())
}

fn terminal_write(call: &NativeCall<'_>, operation: &str, bytes: &[u8]) -> Result<crate::Value> {
    let mut output = io::stdout().lock();
    output.write_all(bytes).and_then(|_| output.flush())
        .map_err(|error| call.error(format!("LengText.{operation} failed: {error}")))?;
    Ok(call.unit_value())
}

// Repaint from cursor-home in one locked write. The trailing erase removes
// remnants of a previously taller frame without exposing a cleared screen.
fn blit(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(1, "LengText.blit")?;
    let frame = call.string(0, "LengText.blit")?;
    // Some terminals treat LF as vertical motion only. Normalize frame rows
    // to CRLF so every line begins at column one, regardless of tty mode.
    let frame = frame.replace("\r\n", "\n").replace('\n', "\r\n");
    let bytes = format!("\x1b[H{frame}\x1b[J");
    terminal_write(&call, "blit", bytes.as_bytes())
}

fn screen_begin(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "LengText.screen_begin")?;
    let result = terminal_write(&call, "screen_begin", b"\x1b[2J\x1b[H\x1b[?25l")?;
    SCREEN_ACTIVE.with(|active| active.set(true));
    Ok(result)
}

fn screen_end(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "LengText.screen_end")?;
    let result = terminal_write(&call, "screen_end", b"\x1b[?25h")?;
    SCREEN_ACTIVE.with(|active| active.set(false));
    Ok(result)
}

fn size(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "LengText.size")?;
    #[cfg(unix)]
    {
        let fd = if unsafe { libc::isatty(libc::STDOUT_FILENO) } == 1 {
            libc::STDOUT_FILENO
        } else {
            libc::STDIN_FILENO
        };
        let mut dimensions = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut dimensions) } == -1 {
            return Err(call.error(format!(
                "LengText.size failed: {}",
                io::Error::last_os_error()
            )));
        }
        if dimensions.ws_col == 0 || dimensions.ws_row == 0 {
            return Err(call.error("LengText.size failed: terminal dimensions are unavailable"));
        }
        let values = vec![
            val(Ty::Int, Data::Int(i64::from(dimensions.ws_col))),
            val(Ty::Int, Data::Int(i64::from(dimensions.ws_row))),
        ];
        Ok(val(
            Ty::List(Box::new(Ty::Int)),
            Data::List(Rc::new(RefCell::new(values))),
        ))
    }
    #[cfg(not(unix))]
    {
        Err(call.error("LengText.size is currently supported on Unix terminals"))
    }
}

fn restore_screen(output: &mut impl Write) -> io::Result<()> {
    if SCREEN_ACTIVE.with(|active| active.replace(false)) {
        output.write_all(b"\x1b[?25h")?;
        output.flush()?;
    }
    Ok(())
}

fn shutdown() {
    let mut output = io::stdout().lock();
    let _ = restore_screen(&mut output);
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

fn palette(call: NativeCall<'_>) -> Result<crate::Value> {
    call.exactly(0, "LengText.palette")?;

    let colours = [
        ("black", "30"),
        ("red", "31"),
        ("green", "32"),
        ("yellow", "33"),
        ("blue", "34"),
        ("magenta", "35"),
        ("cyan", "36"),
        ("white", "37"),
        ("grey", "90"),
        ("bright_red", "91"),
        ("bright_green", "92"),
        ("bright_yellow", "93"),
        ("bright_blue", "94"),
        ("bright_magenta", "95"),
        ("bright_cyan", "96"),
        ("bright_white", "97"),
        ("orange", "38;5;208"),
        ("pink", "38;5;213"),
        ("purple", "38;5;129"),
        ("violet", "38;5;177"),
        ("teal", "38;5;37"),
        ("lime", "38;5;118"),
        ("gold", "38;5;220"),
        ("sky", "38;5;117"),
    ];

    println!("LengText palette\n");

    for (name, ansi) in colours {
        println!("  \x1b[{ansi}m████████\x1b[0m  \x1b[{ansi}m{name}\x1b[0m");
    }

    io::stdout()
        .flush()
        .map_err(|error| call.error(format!("LengText.palette failed: {error}")))?;

    Ok(call.unit_value())
}

#[cfg(test)]
mod tests {
    use super::{restore_screen, SCREEN_ACTIVE};

    #[test]
    fn cleanup_restores_a_hidden_cursor_once() {
        SCREEN_ACTIVE.with(|active| active.set(true));
        let mut output = Vec::new();

        restore_screen(&mut output).unwrap();
        restore_screen(&mut output).unwrap();

        assert_eq!(output, b"\x1b[?25h");
        assert!(!SCREEN_ACTIVE.with(|active| active.get()));
    }
}
