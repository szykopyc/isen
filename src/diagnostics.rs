use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, check_file};

const FORMAT: &str = "isen-diagnostics-v1";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    path: PathBuf,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    severity: &'static str,
    message: String,
}

pub(crate) struct Report {
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub(crate) fn check(files: &[PathBuf]) -> Self {
        let mut diagnostics = Vec::new();
        for file in files {
            if let Err(error) = check_file(file) {
                let diagnostic = Diagnostic::from_error(file, error);
                if !diagnostics.contains(&diagnostic) {
                    diagnostics.push(diagnostic);
                }
            }
        }
        diagnostics.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.line.cmp(&right.line))
                .then(left.column.cmp(&right.column))
                .then(left.message.cmp(&right.message))
        });
        Self { diagnostics }
    }

    pub(crate) fn is_clean(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub(crate) fn json(&self) -> String {
        let mut output = format!("{{\n  \"format\": \"{FORMAT}\",\n  \"diagnostics\": [");
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            output.push_str(&format!(
                "\n    {{\"path\": \"{}\", \"line\": {}, \"column\": {}, \"end_line\": {}, \"end_column\": {}, \"severity\": \"{}\", \"message\": \"{}\"}}",
                escape_json(&diagnostic.path.to_string_lossy()),
                diagnostic.line,
                diagnostic.column,
                diagnostic.end_line,
                diagnostic.end_column,
                diagnostic.severity,
                escape_json(&diagnostic.message),
            ));
        }
        if !self.diagnostics.is_empty() {
            output.push('\n');
            output.push_str("  ");
        }
        output.push_str("]\n}\n");
        output
    }
}

impl Diagnostic {
    fn from_error(default_path: &Path, error: Error) -> Self {
        let path = error.source.unwrap_or_else(|| default_path.to_owned());
        let path = fs::canonicalize(&path).unwrap_or(path);
        let line = error.line.max(1);
        let end_column = fs::read_to_string(&path)
            .ok()
            .and_then(|source| {
                source
                    .lines()
                    .nth(line - 1)
                    .map(|line| line.chars().count() + 1)
            })
            .unwrap_or(1);
        Self {
            path,
            line,
            column: 1,
            end_line: line,
            end_column,
            severity: "error",
            message: error.message,
        }
    }
}

fn escape_json(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_versioned_and_escapes_messages() {
        let report = Report {
            diagnostics: vec![Diagnostic {
                path: PathBuf::from("a.is"),
                line: 2,
                column: 1,
                end_line: 2,
                end_column: 4,
                severity: "error",
                message: "bad \"thing\"\nnext".into(),
            }],
        };
        let json = report.json();
        assert!(json.contains("\"format\": \"isen-diagnostics-v1\""));
        assert!(json.contains("bad \\\"thing\\\"\\nnext"));
    }
}
