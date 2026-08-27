use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, Result, execute_file};

pub(crate) struct Summary {
    pub passed: usize,
    pub failed: usize,
}

#[cfg(test)]
pub(crate) fn run(inputs: &[PathBuf]) -> Result<Summary> {
    run_with_options(inputs, false)
}

pub(crate) fn run_with_options(inputs: &[PathBuf], fail_fast: bool) -> Result<Summary> {
    let files = collect(inputs)?;
    println!("running {} Isen tests", files.len());
    let mut summary = Summary {
        passed: 0,
        failed: 0,
    };
    for path in files {
        match execute_file(&path) {
            Ok(()) => {
                summary.passed += 1;
                println!("test {} ... ok", path.display());
            }
            Err(error) if error.clean_exit => {
                summary.passed += 1;
                println!("test {} ... ok", path.display());
            }
            Err(error) => {
                summary.failed += 1;
                println!("test {} ... FAILED", path.display());
                eprintln!("{error}");
                if fail_fast {
                    break;
                }
            }
        }
    }
    println!(
        "test result: {}. {} passed; {} failed",
        if summary.failed == 0 { "ok" } else { "FAILED" },
        summary.passed,
        summary.failed,
    );
    Ok(summary)
}

fn collect(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        let metadata = fs::metadata(input)
            .map_err(|error| Error::new(0, error.to_string()).with_source(input))?;
        if metadata.is_file() {
            if input.extension().is_none_or(|extension| extension != "is") {
                return Err(
                    Error::new(0, "test files must use the .is extension").with_source(input)
                );
            }
            files.push(input.clone());
        } else if metadata.is_dir() {
            collect_directory(input, &mut files)?;
        } else {
            return Err(Error::new(0, "test path is not a file or directory").with_source(input));
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
        return Err(Error::new(
            0,
            "no tests found (directory tests must end in .test.is)",
        ));
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
            if !name.starts_with('.') && name != "target" && !path.join(".git").exists() {
                collect_directory(&path, files)?;
            }
        } else if file_type.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".test.is"))
        {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_discovery_only_selects_test_programmes() {
        let root = std::env::temp_dir().join(format!("isen-test-runner-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("one.test.is"), "say(1)").unwrap();
        fs::write(root.join("ignored.is"), "say(2)").unwrap();
        fs::write(root.join("nested/two.test.is"), "say(3)").unwrap();
        let files = collect(std::slice::from_ref(&root)).unwrap();
        assert_eq!(files.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shipped_language_tests_pass() {
        let summary = run(&[PathBuf::from("tests/stdlib.test.is")]).unwrap();
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.passed, 1);
    }
}
