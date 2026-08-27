use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, Parser, Result, lex, project::ProjectConfig};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Word,
    Number,
    Text,
    Symbol,
    Comment,
}

#[derive(Clone, Debug)]
struct Piece {
    kind: Kind,
    text: String,
}

impl Piece {
    fn symbol(&self, symbol: &str) -> bool {
        self.kind == Kind::Symbol && self.text == symbol
    }
}

#[cfg(test)]
pub(crate) fn format_source(source: &str) -> Result<String> {
    format_source_with(source, &ProjectConfig::default())
}

fn format_source_with(source: &str, config: &ProjectConfig) -> Result<String> {
    // Never turn a malformed file into a different malformed file. Formatting
    // deliberately does not type-check, so it remains useful while editing.
    let original_tokens = lex(source)?;
    Parser::new(original_tokens.clone()).program()?;

    let mut output = String::new();
    let mut block_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut blank_lines = 0usize;

    for raw_line in source.lines() {
        let pieces = pieces(raw_line);
        if pieces.is_empty() {
            if blank_lines < config.max_blank_lines {
                output.push('\n');
            }
            blank_lines += 1;
            continue;
        }
        blank_lines = 0;

        let leading_blocks = pieces
            .iter()
            .take_while(|piece| piece.symbol("\\$"))
            .count();
        let leading_brackets = pieces
            .iter()
            .take_while(|piece| piece.symbol(")") || piece.symbol("]") || piece.symbol("}"))
            .count();
        let line_block_depth = block_depth.saturating_sub(leading_blocks);
        let line_bracket_depth = bracket_depth.saturating_sub(leading_brackets);
        let continuation = usize::from(line_bracket_depth > 0);
        let leading_operator = usize::from(
            line_bracket_depth > 0 && pieces.first().is_some_and(is_continuation_operator),
        );

        output.push_str(
            &" ".repeat(config.indent_width * (line_block_depth + continuation + leading_operator)),
        );
        output.push_str(&render(&pieces));
        output.push('\n');

        for piece in &pieces {
            if piece.kind == Kind::Comment {
                break;
            }
            if piece.symbol("$") {
                block_depth += 1;
            } else if piece.symbol("\\$") {
                block_depth = block_depth.saturating_sub(1);
            } else if is_open_bracket(piece) {
                bracket_depth += 1;
            } else if is_close_bracket(piece) {
                bracket_depth = bracket_depth.saturating_sub(1);
            }
        }
    }

    if !source.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    // `lines` omits the final empty segment, so finish every non-empty file
    // with exactly one newline.
    let allowed_newlines = config.max_blank_lines + 1;
    while output.ends_with(&"\n".repeat(allowed_newlines + 1)) {
        output.pop();
    }
    if !config.final_newline {
        output.pop();
    }

    let formatted_tokens = lex(&output)?;
    let same_tokens = original_tokens
        .iter()
        .map(|spanned| &spanned.token)
        .eq(formatted_tokens.iter().map(|spanned| &spanned.token));
    if !same_tokens {
        return Err(Error::new(1, "formatter changed the programme's tokens"));
    }
    Parser::new(formatted_tokens).program()?;
    Ok(output)
}

pub(crate) fn format_file(path: &Path) -> Result<bool> {
    let source = fs::read_to_string(path)
        .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
    let config = ProjectConfig::discover(path)?;
    let formatted =
        format_source_with(&source, &config).map_err(|error| error.with_source(path))?;
    if formatted == source {
        return Ok(false);
    }
    fs::write(path, formatted)
        .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
    Ok(true)
}

pub(crate) fn is_formatted(path: &Path) -> Result<bool> {
    let source = fs::read_to_string(path)
        .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
    let config = ProjectConfig::discover(path)?;
    let formatted =
        format_source_with(&source, &config).map_err(|error| error.with_source(path))?;
    Ok(formatted == source)
}

pub(crate) fn collect_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        let metadata = fs::metadata(input)
            .map_err(|error| Error::new(0, error.to_string()).with_source(input))?;
        if metadata.is_file() {
            files.push(input.clone());
        } else if metadata.is_dir() {
            collect_directory(input, &mut files)?;
        } else {
            return Err(Error::new(0, "path is not a file or directory").with_source(input));
        }
    }

    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for path in files {
        let canonical = fs::canonicalize(&path)
            .map_err(|error| Error::new(0, error.to_string()).with_source(&path))?;
        if seen.insert(canonical) {
            unique.push(path);
        }
    }
    unique.sort();
    if unique.is_empty() {
        let source = inputs
            .first()
            .map(PathBuf::as_path)
            .unwrap_or(Path::new("."));
        return Err(Error::new(0, "no .is files found").with_source(source));
    }
    Ok(unique)
}

fn collect_directory(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| Error::new(0, error.to_string()).with_source(directory))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| Error::new(0, error.to_string()).with_source(directory))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| Error::new(0, error.to_string()).with_source(&path))?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" || path.join(".git").exists() {
                continue;
            }
            collect_directory(&path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "is")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn pieces(line: &str) -> Vec<Piece> {
    let characters = line.char_indices().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut at = 0usize;

    while at < characters.len() {
        let (start, character) = characters[at];
        if character.is_whitespace() {
            at += 1;
            continue;
        }

        if character == '"' {
            let mut end = line.len();
            let mut escaped = false;
            at += 1;
            while at < characters.len() {
                let (index, current) = characters[at];
                at += 1;
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == '"' {
                    end = index + current.len_utf8();
                    break;
                }
            }
            output.push(Piece {
                kind: Kind::Text,
                text: line[start..end].to_owned(),
            });
            continue;
        }

        if character == '/' && next_character(&characters, at) == Some('/')
            || character == '#' && next_character(&characters, at) != Some('{')
        {
            output.push(Piece {
                kind: Kind::Comment,
                text: line[start..].trim_end().to_owned(),
            });
            break;
        }

        if character.is_ascii_alphabetic() || character == '_' {
            let mut end = start + character.len_utf8();
            at += 1;
            while at < characters.len() {
                let (index, current) = characters[at];
                if !(current.is_ascii_alphanumeric() || current == '_') {
                    break;
                }
                end = index + current.len_utf8();
                at += 1;
            }
            output.push(Piece {
                kind: Kind::Word,
                text: line[start..end].to_owned(),
            });
            continue;
        }

        if character.is_ascii_digit() {
            let mut end = start + 1;
            at += 1;
            while at < characters.len() {
                let (index, current) = characters[at];
                if !(current.is_ascii_digit() || current == '.') {
                    break;
                }
                end = index + current.len_utf8();
                at += 1;
            }
            output.push(Piece {
                kind: Kind::Number,
                text: line[start..end].to_owned(),
            });
            continue;
        }

        let next = next_character(&characters, at);
        let third = characters.get(at + 2).map(|(_, character)| *character);
        if matches!(
            (character, next, third),
            ('<', Some('<'), Some('=')) | ('>', Some('>'), Some('='))
        ) {
            output.push(Piece {
                kind: Kind::Symbol,
                text: format!("{character}{character}="),
            });
            at += 3;
            continue;
        }
        let paired = match (character, next) {
            ('@', Some('@')) => Some("@@"),
            ('\\', Some('$')) => Some("\\$"),
            ('@', Some('[')) => Some("@["),
            ('#', Some('{')) => Some("#{"),
            ('=', Some('=')) => Some("=="),
            ('!', Some('=')) => Some("!="),
            ('<', Some('=')) => Some("<="),
            ('>', Some('=')) => Some(">="),
            ('&', Some('&')) => Some("&&"),
            ('|', Some('|')) => Some("||"),
            ('<', Some('<')) => Some("<<"),
            ('>', Some('>')) => Some(">>"),
            ('+', Some('=')) => Some("+="),
            ('-', Some('=')) => Some("-="),
            ('*', Some('=')) => Some("*="),
            ('/', Some('=')) => Some("/="),
            ('%', Some('=')) => Some("%="),
            ('&', Some('=')) => Some("&="),
            ('|', Some('=')) => Some("|="),
            ('^', Some('=')) => Some("^="),
            _ => None,
        };
        if let Some(symbol) = paired {
            output.push(Piece {
                kind: Kind::Symbol,
                text: symbol.to_owned(),
            });
            at += 2;
        } else {
            output.push(Piece {
                kind: Kind::Symbol,
                text: character.to_string(),
            });
            at += 1;
        }
    }

    output
}

fn next_character(characters: &[(usize, char)], at: usize) -> Option<char> {
    characters.get(at + 1).map(|(_, character)| *character)
}

fn render(pieces: &[Piece]) -> String {
    let mut output = String::new();
    for (index, piece) in pieces.iter().enumerate() {
        if index > 0 {
            let previous = &pieces[index - 1];
            if piece.kind == Kind::Comment {
                output.push_str("  ");
            } else if needs_space(pieces, index, previous, piece) {
                output.push(' ');
            }
        }
        output.push_str(&piece.text);
    }
    output
}

fn needs_space(pieces: &[Piece], index: usize, previous: &Piece, current: &Piece) -> bool {
    if current.symbol(")")
        || current.symbol("]")
        || current.symbol("}")
        || current.symbol(",")
        || current.symbol(";")
        || current.symbol(".")
        || current.symbol(":")
    {
        return false;
    }
    if previous.symbol("(")
        || previous.symbol("[")
        || previous.symbol("@[")
        || previous.symbol("#{")
        || previous.symbol(".")
    {
        return false;
    }
    if current.symbol("[") {
        return !can_end_postfix_target(previous);
    }
    if current.symbol("(") {
        return !can_end_postfix_target(previous) && !previous.symbol("!");
    }
    if previous.symbol(",") || previous.symbol(":") || previous.symbol(";") {
        return true;
    }
    if is_operator(previous) || is_operator(current) {
        if is_unary(pieces, index.saturating_sub(1), previous) {
            return false;
        }
        return true;
    }
    true
}

fn can_end_postfix_target(piece: &Piece) -> bool {
    matches!(piece.kind, Kind::Number | Kind::Text)
        || piece.kind == Kind::Word
            && !matches!(
                piece.text.as_str(),
                "if" | "aslongas" | "each" | "in" | "ret" | "dec"
            )
        || piece.symbol(")")
        || piece.symbol("]")
        || piece.symbol("}")
}

fn is_operator(piece: &Piece) -> bool {
    piece.kind == Kind::Symbol
        && matches!(
            piece.text.as_str(),
            "=" | "@@"
                | "+="
                | "-="
                | "*="
                | "/="
                | "%="
                | "&="
                | "|="
                | "^="
                | "<<="
                | ">>="
                | "+"
                | "-"
                | "*"
                | "/"
                | "%"
                | "!"
                | "=="
                | "!="
                | "<"
                | "<="
                | ">"
                | ">="
                | "&&"
                | "||"
                | "&"
                | "|"
                | "^"
                | "<<"
                | ">>"
        )
}

fn is_continuation_operator(piece: &Piece) -> bool {
    piece.kind == Kind::Symbol
        && matches!(
            piece.text.as_str(),
            "+" | "-" | "*" | "/" | "%" | "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"
        )
}

fn is_unary(pieces: &[Piece], index: usize, piece: &Piece) -> bool {
    if !(piece.symbol("-") || piece.symbol("!")) {
        return false;
    }
    if index == 0 {
        return true;
    }
    let previous = &pieces[index - 1];
    is_operator(previous)
        || matches!(previous.text.as_str(), "if" | "aslongas" | "ret" | "in")
        || previous.symbol("(")
        || previous.symbol("[")
        || previous.symbol("@[")
        || previous.symbol("#{")
        || previous.symbol(",")
        || previous.symbol(":")
        || previous.symbol("$")
}

fn is_open_bracket(piece: &Piece) -> bool {
    piece.symbol("(") || piece.symbol("[") || piece.symbol("@[") || piece.symbol("#{")
}

fn is_close_bracket(piece: &Piece) -> bool {
    piece.symbol(")") || piece.symbol("]") || piece.symbol("}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("isen-formatter-{}-{unique}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn formats_spacing_indentation_and_comments() {
        let source = concat!(
            "given add( left@@int,right @@ int)@@int $\n",
            "# retain this comment\n",
            "dec total@@int=left+right // and this one\n",
            "if(total>0)$ ret -total \\$\n",
            "ret total\n",
            "\\$"
        );
        let expected = concat!(
            "given add(left @@ int, right @@ int) @@ int $\n",
            "  # retain this comment\n",
            "  dec total @@ int = left + right  // and this one\n",
            "  if (total > 0) $ ret -total \\$\n",
            "  ret total\n",
            "\\$\n"
        );
        assert_eq!(format_source(source).unwrap(), expected);
    }

    #[test]
    fn formats_explicit_generic_parameters_as_part_of_the_function_name() {
        let source = "given first [ T, U, ](values@@list[T])@@T $ ret values[0] \\$";
        let expected = "given first[T, U,](values @@ list[T]) @@ T $ ret values[0] \\$\n";
        assert_eq!(format_source(source).unwrap(), expected);
    }

    #[test]
    fn preserves_compound_assignment_tokens() {
        let source = concat!(
            "dec value@@int=1\n",
            "value+=1\nvalue-=1\nvalue*=2\nvalue/=2\nvalue%=2\n",
            "value&=1\nvalue|=1\nvalue^=1\nvalue<<=1\nvalue>>=1\n"
        );
        let formatted = format_source(source).unwrap();
        assert!(formatted.contains("value += 1"));
        assert!(formatted.contains("value <<= 1"));
        assert_eq!(format_source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn preserves_literal_spelling_and_comment_markers() {
        let source = concat!(
            "dec values=@[1.0,-2]\n",
            "dec nested=[[1],[2]]\n",
            "dec first=nested[0]\n",
            "dec label=\"# not // comments\" # real\n"
        );
        let formatted = format_source(source).unwrap();
        assert_eq!(
            formatted,
            concat!(
                "dec values = @[1.0, -2]\n",
                "dec nested = [[1], [2]]\n",
                "dec first = nested[0]\n",
                "dec label = \"# not // comments\"  # real\n"
            )
        );
    }

    #[test]
    fn distinguishes_calls_and_indexes_from_grouping_and_literals() {
        let source = concat!(
            "dec value=call((left+right),[1,2])\n",
            "if(value>0)$ ret !(value==items[0]) \\$\n"
        );
        assert_eq!(
            format_source(source).unwrap(),
            concat!(
                "dec value = call((left + right), [1, 2])\n",
                "if (value > 0) $ ret !(value == items[0]) \\$\n"
            )
        );
    }

    #[test]
    fn formatting_is_idempotent() {
        let source = "if true $\n say(\"yes\")\n\\$ else $ say(\"no\") \\$\n";
        let once = format_source(source).unwrap();
        assert_eq!(format_source(&once).unwrap(), once);
    }

    #[test]
    fn formats_the_is_programme_corpus_idempotently() {
        fn visit(path: &Path) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path);
                } else if path.extension().is_some_and(|extension| extension == "is") {
                    let source = fs::read_to_string(&path).unwrap();
                    let formatted = format_source(&source)
                        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                    assert_eq!(
                        format_source(&formatted).unwrap(),
                        formatted,
                        "{} is not idempotent",
                        path.display()
                    );
                }
            }
        }

        visit(Path::new("examples"));
        visit(Path::new("stdlib"));
        visit(Path::new("tests"));
        visit(Path::new("examples/labyrinth"));
    }

    #[test]
    fn discovers_paths_recursively_and_deduplicates_overlaps() {
        let directory = TestDirectory::new();
        let nested = directory.path.join("nested");
        let hidden = directory.path.join(".hidden");
        let target = directory.path.join("target");
        fs::create_dir(&nested).unwrap();
        fs::create_dir(&hidden).unwrap();
        fs::create_dir(&target).unwrap();

        let first = directory.path.join("first.is");
        let second = nested.join("second.is");
        fs::write(&first, "say(1)").unwrap();
        fs::write(&second, "say(2)").unwrap();
        fs::write(directory.path.join("notes.txt"), "ignored").unwrap();
        fs::write(hidden.join("hidden.is"), "say(3)").unwrap();
        fs::write(target.join("generated.is"), "say(4)").unwrap();

        let files = collect_files(&[directory.path.clone(), first.clone()]).unwrap();
        assert_eq!(files, vec![first, second]);
    }
}
