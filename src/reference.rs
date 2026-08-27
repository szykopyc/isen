use std::{fs, path::Path};

use crate::{
    BINARY_OPERATOR_GROUPS, Error, Result, SOURCE_LEAF_TYPES,
    native::{NativeExpected, NativeProduced, NativeRegistry},
};

const TYPES_START: &str = "(* BEGIN GENERATED:SOURCE_TYPES *)";
const TYPES_END: &str = "(* END GENERATED:SOURCE_TYPES *)";
const OPERATORS_START: &str = "<!-- BEGIN GENERATED:OPERATORS -->";
const OPERATORS_END: &str = "<!-- END GENERATED:OPERATORS -->";
const NATIVE_START: &str = "<!-- BEGIN GENERATED:NATIVE_API -->";
const NATIVE_END: &str = "<!-- END GENERATED:NATIVE_API -->";

pub(crate) fn synchronize(path: &Path, check: bool) -> Result<bool> {
    let original = fs::read_to_string(path)
        .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
    let mut rendered = replace_section(
        &original,
        TYPES_START,
        TYPES_END,
        &render_source_types(),
        path,
    )?;
    rendered = replace_section(
        &rendered,
        OPERATORS_START,
        OPERATORS_END,
        &render_operators(),
        path,
    )?;
    rendered = replace_section(
        &rendered,
        NATIVE_START,
        NATIVE_END,
        &render_native_api(),
        path,
    )?;
    if rendered == original {
        return Ok(false);
    }
    if check {
        return Err(Error::new(
            0,
            "generated reference sections are stale; run 'isen --reference'",
        )
        .with_source(path));
    }
    fs::write(path, rendered)
        .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
    Ok(true)
}

fn replace_section(
    document: &str,
    start: &str,
    end: &str,
    generated: &str,
    path: &Path,
) -> Result<String> {
    let start_at = document.find(start).ok_or_else(|| {
        Error::new(0, format!("reference marker {start:?} is missing")).with_source(path)
    })?;
    let content_at = start_at + start.len();
    let relative_end = document[content_at..].find(end).ok_or_else(|| {
        Error::new(0, format!("reference marker {end:?} is missing")).with_source(path)
    })?;
    let end_at = content_at + relative_end;
    let mut output = String::with_capacity(document.len() + generated.len());
    output.push_str(&document[..content_at]);
    output.push('\n');
    output.push_str(generated.trim_end());
    output.push('\n');
    output.push_str(&document[end_at..]);
    Ok(output)
}

fn render_source_types() -> String {
    let leaves = SOURCE_LEAF_TYPES
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "type          = {leaves} | IDENT\n\
         \x20             | \"perchance\" \"[\" type \"]\"\n\
         \x20             | \"list\" \"[\" type \"]\"\n\
         \x20             | \"arr\" \"[\" type \"]\"\n\
         \x20             | \"map\" \"[\" type \",\" type \"]\" ;"
    )
}

fn render_operators() -> String {
    let mut output =
        String::from("| Precedence | Operators | Valid operands |\n| --- | --- | --- |\n");
    for group in BINARY_OPERATOR_GROUPS {
        let operators = group
            .operators
            .iter()
            .map(|operator| format!("`{}`", operator.replace('|', "\\|")))
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "| {} | {} | {} |\n",
            group.precedence, operators, group.operands
        ));
    }
    output.push_str(
        "| unary | `-`, `!`, `~` | numeric negation; boolean negation; integer complement |\n",
    );
    output
}

fn render_native_api() -> String {
    let mut registry = NativeRegistry::metadata_only();
    crate::extensions::register_all(&mut registry);
    let metadata = registry.into_metadata();
    let mut output = String::from(
        "This catalog is generated from the native extension registry. Argument names are\n\
         positional (`arg1`, `arg2`, …); package guidance above provides descriptive names.\n\
         Angle-bracketed constraints and `T1`/`K1`/`V1` are documentation metavariables,\n\
         not source-level type names.\n",
    );
    for (space, package) in metadata {
        if space == "ML" {
            continue;
        }
        output.push_str(&format!("\n### `{space}` generated surface\n\n```text\n"));
        for signature in package.functions {
            let parameters = signature
                .parameters
                .iter()
                .enumerate()
                .map(|(index, expected)| {
                    format!("arg{} @@ {}", index + 1, render_expected(expected, index))
                })
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!(
                "{space}.{}({parameters}) -> {}\n",
                signature.name,
                render_produced(&signature.result)
            ));
        }
        for (name, ty) in package.constants {
            output.push_str(&format!("{space}.{name} @@ {ty}\n"));
        }
        output.push_str("```\n");
    }
    output
}

fn render_expected(expected: &NativeExpected, index: usize) -> String {
    match expected {
        NativeExpected::Exact(ty) => ty.to_string(),
        NativeExpected::Any => "<any value>".into(),
        NativeExpected::Number => "<int or float>".into(),
        NativeExpected::Ordered => "<int, float, or string>".into(),
        NativeExpected::List => format!("list[T{}]", index + 1),
        NativeExpected::Map => format!("map[K{}, V{}]", index + 1, index + 1),
        NativeExpected::SameAs(other) => format!("<same type as arg{}>", other + 1),
    }
}

fn render_produced(produced: &NativeProduced) -> String {
    match produced {
        NativeProduced::Exact(ty) => ty.to_string(),
        NativeProduced::SameAs(index) => format!("<same type as arg{}>", index + 1),
        NativeProduced::OptionalListElement(index) => {
            format!("perchance[element_of_arg{}]", index + 1)
        }
        NativeProduced::OptionalMapValue(index) => {
            format!("perchance[value_of_arg{}]", index + 1)
        }
        NativeProduced::MapKeys(index) => format!("list[key_of_arg{}]", index + 1),
        NativeProduced::ArrayOfArgument(index) => format!("arr[type_of_arg{}]", index + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_reference_has_current_generated_sections() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/LANGUAGE_REFERENCE.md");
        synchronize(&path, true).unwrap();
    }

    #[test]
    fn generated_types_include_every_parser_leaf_type() {
        let rendered = render_source_types();
        for (name, _) in SOURCE_LEAF_TYPES {
            assert!(rendered.contains(&format!("\"{name}\"")));
        }
        assert!(rendered.contains("\"json\""));
    }

    #[test]
    fn generated_operator_table_escapes_markdown_pipes() {
        let rendered = render_operators();
        assert!(rendered.contains("`\\|\\|`"));
        assert!(rendered.contains("`\\|`"));
        assert!(!rendered.contains("| `||` |"));
    }
}
