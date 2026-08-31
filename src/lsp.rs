use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
};

use serde_json::{Value, json};

pub(crate) fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut output = io::stdout().lock();
    let mut documents = HashMap::<String, String>::new();

    while let Some(message) = read_message(&mut input)? {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        if method == "exit" {
            break;
        }
        if let Some(document) = message.pointer("/params/textDocument") {
            if let (Some(uri), Some(text)) = (
                document.get("uri").and_then(Value::as_str),
                document.get("text").and_then(Value::as_str),
            ) {
                documents.insert(uri.into(), text.into());
            }
        }
        if method == "textDocument/didChange"
            && let (Some(uri), Some(text)) = (
                message
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str),
                message
                    .pointer("/params/contentChanges/0/text")
                    .and_then(Value::as_str),
            )
        {
            documents.insert(uri.into(), text.into());
        }

        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        let result = match method {
            "initialize" => json!({
                "capabilities": {
                    "hoverProvider": true,
                    "textDocumentSync": 1
                },
                "serverInfo": { "name": "isen", "version": env!("CARGO_PKG_VERSION") }
            }),
            "shutdown" => Value::Null,
            "textDocument/hover" => {
                let uri = message
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let line = message
                    .pointer("/params/position/line")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let character = message
                    .pointer("/params/position/character")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                documents
                    .get(uri)
                    .and_then(|source| hover(source, line, character, Some(uri)))
                    .map_or(
                        Value::Null,
                        |text| json!({ "contents": { "kind": "markdown", "value": text } }),
                    )
            }
            _ => Value::Null,
        };
        write_message(
            &mut output,
            &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        )?;
    }
    Ok(())
}

fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.ok_or_else(|| io::Error::other("missing Content-Length"))?];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(io::Error::other)
}

fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = message.to_string();
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    output.flush()
}

fn hover(source: &str, line: usize, utf16_column: usize, uri: Option<&str>) -> Option<String> {
    let current = source.lines().nth(line)?;
    let byte = utf16_byte(current, utf16_column);
    let (start, end) = word_bounds(current, byte)?;
    let mut name = &current[start..end];
    let qualified_start = current[..start]
        .char_indices()
        .rev()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.')
        })
        .last()
        .map_or(start, |(index, _)| index);
    if current[qualified_start..end].contains('.') {
        name = &current[qualified_start..end];
    }

    if let Some((signature, documentation)) = crate::native::hover_signature(name) {
        return Some(format!("```isen\n{signature}\n```\n\n{documentation}"));
    }
    let short_name = name.rsplit('.').next().unwrap_or(name);
    declaration(source, short_name)
        .or_else(|| imported_declaration(source, short_name, uri?))
        .map(|(signature, docs)| {
            let mut value = format!("```isen\n{signature}\n```");
            if !docs.is_empty() {
                value.push_str("\n\n");
                value.push_str(&docs);
            }
            value
        })
}

fn imported_declaration(source: &str, name: &str, uri: &str) -> Option<(String, String)> {
    let path = uri.strip_prefix("file://")?.replace("%20", " ");
    for line in source.lines() {
        let Some(rest) = line.trim().strip_prefix("borrow ") else {
            continue;
        };
        let Some(borrowed) = rest.split_whitespace().next() else {
            continue;
        };
        let alias = rest.split_once(" as ").map(|(_, alias)| alias.trim());
        if name != borrowed && alias != Some(name) {
            continue;
        }
        let (_, quoted) = rest.split_once(" from \"")?;
        let requested = quoted.split_once('"')?.0;
        let resolved = crate::project::resolve_stash(
            std::path::Path::new(&path),
            std::path::Path::new(&path),
            requested,
        )
        .ok()?;
        let imported = std::fs::read_to_string(resolved).ok()?;
        return declaration(&imported, borrowed);
    }
    None
}

fn utf16_byte(line: &str, column: usize) -> usize {
    let mut units = 0;
    for (byte, character) in line.char_indices() {
        if units >= column {
            return byte;
        }
        units += character.len_utf16();
    }
    line.len()
}

fn word_bounds(line: &str, at: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut start = at.min(bytes.len());
    if start == bytes.len() || !bytes[start].is_ascii_alphanumeric() && bytes[start] != b'_' {
        start = start.checked_sub(1)?;
    }
    if !bytes[start].is_ascii_alphanumeric() && bytes[start] != b'_' {
        return None;
    }
    let mut end = start + 1;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    Some((start, end))
}

fn declaration(source: &str, name: &str) -> Option<(String, String)> {
    let lines = source.lines().collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let declares = ["given ", "dec ", "form ", "problem ", "space "]
            .iter()
            .any(|prefix| {
                trimmed.strip_prefix(prefix).is_some_and(|rest| {
                    rest.strip_prefix(name).is_some_and(|after| {
                        after.is_empty()
                            || after.starts_with(|character: char| {
                                !character.is_ascii_alphanumeric() && character != '_'
                            })
                    })
                })
            })
            || trimmed
                .strip_prefix(name)
                .is_some_and(|rest| rest.trim_start().starts_with("@@"));
        if declares {
            return Some((
                trimmed.trim_end_matches('$').trim().into(),
                docs_before(&lines, index),
            ));
        }
    }
    None
}

fn docs_before(lines: &[&str], mut index: usize) -> String {
    let mut docs = Vec::new();
    while index > 0 {
        let line = lines[index - 1].trim_start();
        let Some(text) = line.strip_prefix("///") else {
            break;
        };
        docs.push(text.strip_prefix(' ').unwrap_or(text));
        index -= 1;
    }
    docs.reverse();
    // Markdown collapses ordinary newlines. A trailing pair of spaces keeps
    // consecutive documentation lines visually separate in hover clients.
    docs.join("  \n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn hovers_documented_user_functions_and_native_functions() {
        let source = "/// Says hello.\n/// Uses the supplied name.\ngiven hello() @@ unit $\n  say(\"hello\")\n\\$\nhello()\nborrow Maths\nMaths.sqrt(4.0)\n";
        let user = hover(source, 5, 2, None).unwrap();
        assert!(user.contains("given hello() @@ unit"));
        assert!(user.contains("Says hello.  \nUses the supplied name."));
        let native = hover(source, 7, 8, None).unwrap();
        assert!(native.contains("Maths.sqrt"));
        assert!(native.contains("square root"));
    }

    #[test]
    fn hovers_documented_borrowed_functions() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("isen-lsp-{unique}"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("library.is"),
            "/// Returns the answer.\ngiven answer() @@ int $ ret 42 \\$\nshare answer\n",
        )
        .unwrap();
        let programme = directory.join("main.is");
        let source = "borrow answer from \"library.is\"\nsay(answer())\n";
        std::fs::write(&programme, source).unwrap();
        let uri = format!("file://{}", programme.display());

        let result = hover(source, 1, 6, Some(&uri)).unwrap();
        assert!(result.contains("given answer() @@ int"));
        assert!(result.contains("Returns the answer."));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
