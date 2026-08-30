use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use crate::{Error, Result};

#[derive(Clone, Debug)]
pub(crate) struct ProjectConfig {
    pub(crate) indent_width: usize,
    pub(crate) max_blank_lines: usize,
    pub(crate) final_newline: bool,
    pub(crate) stash_links: BTreeMap<String, PathBuf>,
    pub(crate) default_test_profile: Option<String>,
    pub(crate) test_profiles: BTreeMap<String, TestProfile>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TestProfile {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) fail_fast: bool,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            max_blank_lines: 1,
            final_newline: true,
            stash_links: BTreeMap::new(),
            default_test_profile: None,
            test_profiles: BTreeMap::new(),
        }
    }
}

impl ProjectConfig {
    pub(crate) fn discover(source: &Path) -> Result<Self> {
        let start = if source.is_dir() {
            source
        } else {
            source.parent().unwrap_or(Path::new("."))
        };
        let absolute = fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        for directory in absolute.ancestors() {
            let path = directory.join("isen.toml");
            if path.is_file() {
                return Self::read(&path);
            }
        }
        Ok(Self::default())
    }

    fn read(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)
            .map_err(|error| Error::new(0, error.to_string()).with_source(path))?;
        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut config = Self::default();
        let mut section = String::new();
        for (index, raw) in source.lines().enumerate() {
            let line = without_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_owned();
                if !matches!(section.as_str(), "format" | "stash_links" | "test")
                    && test_profile_name(&section).is_none()
                {
                    return Err(Error::new(
                        index + 1,
                        format!("unknown isen.toml section [{section}]"),
                    )
                    .with_source(path));
                }
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| Error::new(index + 1, "expected key = value").with_source(path))?;
            let key = key.trim();
            let value = value.trim();
            match (section.as_str(), key) {
                ("format", "indent_width") => {
                    config.indent_width = integer(value, path, index + 1, 1, 8)?
                }
                ("format", "max_blank_lines") => {
                    config.max_blank_lines = integer(value, path, index + 1, 0, 4)?
                }
                ("format", "final_newline") => {
                    config.final_newline = boolean(value, path, index + 1)?
                }
                ("stash_links", alias) if valid_alias(alias) => {
                    let target = quoted(value, path, index + 1)?;
                    config
                        .stash_links
                        .insert(alias.to_owned(), root.join(target));
                }
                ("test", "default_profile") => {
                    config.default_test_profile = Some(quoted(value, path, index + 1)?.to_owned())
                }
                (profile_section, "paths") if test_profile_name(profile_section).is_some() => {
                    let profile = test_profile_name(profile_section).unwrap();
                    let paths = string_array(value, path, index + 1)?
                        .into_iter()
                        .map(|item| root.join(item))
                        .collect();
                    config
                        .test_profiles
                        .entry(profile.to_owned())
                        .or_default()
                        .paths = paths;
                }
                (profile_section, "fail_fast") if test_profile_name(profile_section).is_some() => {
                    let profile = test_profile_name(profile_section).unwrap();
                    config
                        .test_profiles
                        .entry(profile.to_owned())
                        .or_default()
                        .fail_fast = boolean(value, path, index + 1)?;
                }
                ("", _) => {
                    return Err(Error::new(
                        index + 1,
                        "settings must be inside [format] or [stash_links]",
                    )
                    .with_source(path));
                }
                _ => {
                    return Err(Error::new(
                        index + 1,
                        format!("unknown isen.toml setting {section}.{key}"),
                    )
                    .with_source(path));
                }
            }
        }
        if let Some(default) = &config.default_test_profile {
            if !config.test_profiles.contains_key(default) {
                return Err(Error::new(
                    0,
                    format!("default test profile {default:?} is not defined"),
                )
                .with_source(path));
            }
        }
        for (name, profile) in &config.test_profiles {
            if profile.paths.is_empty() {
                return Err(Error::new(
                    0,
                    format!("test profile {name:?} must define at least one path"),
                )
                .with_source(path));
            }
        }
        Ok(config)
    }

    pub(crate) fn test_profile(
        &self,
        requested: Option<&str>,
    ) -> Result<Option<(&str, &TestProfile)>> {
        let Some(name) = requested.or(self.default_test_profile.as_deref()) else {
            return Ok(None);
        };
        self.test_profiles
            .get_key_value(name)
            .map(|(name, profile)| Some((name.as_str(), profile)))
            .ok_or_else(|| Error::new(0, format!("unknown test profile {name:?}")))
    }
}

pub(crate) fn resolve_stash(
    config_source: &Path,
    importing_source: &Path,
    requested: &str,
) -> Result<PathBuf> {
    let config = ProjectConfig::discover(config_source)?;
    let request = Path::new(requested);
    let mut components = request.components();
    let alias = components.next().and_then(|part| part.as_os_str().to_str());
    let unresolved = if let Some(root) = alias.and_then(|alias| config.stash_links.get(alias)) {
        let canonical_root = fs::canonicalize(root).map_err(|error| {
            Error::new(
                0,
                format!(
                    "could not open linked stash root {}: {error}",
                    root.display()
                ),
            )
        })?;
        let candidate = canonical_root.join(components.as_path());
        let canonical = fs::canonicalize(&candidate).map_err(|error| {
            Error::new(
                0,
                format!("could not open stash {}: {error}", candidate.display()),
            )
        })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(Error::new(
                0,
                format!("stash path {requested:?} escapes its linked root"),
            ));
        }
        return Ok(canonical);
    } else if alias == Some("stdlib") {
        if let Some(result) = bundled_stdlib(request) {
            return result;
        }
        importing_source
            .parent()
            .unwrap_or(Path::new("."))
            .join(request)
    } else {
        importing_source
            .parent()
            .unwrap_or(Path::new("."))
            .join(request)
    };
    fs::canonicalize(&unresolved).map_err(|error| {
        Error::new(
            0,
            format!("could not open stash {}: {error}", unresolved.display()),
        )
    })
}

fn bundled_stdlib(request: &Path) -> Option<Result<PathBuf>> {
    let executable = env::current_exe().ok()?;
    let root = executable.parent()?.join("stdlib");
    root.is_dir()
        .then(|| resolve_bundled_stdlib(&root, request))
}

fn resolve_bundled_stdlib(root: &Path, request: &Path) -> Result<PathBuf> {
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        Error::new(
            0,
            format!("could not open bundled stdlib {}: {error}", root.display()),
        )
    })?;
    let candidate = canonical_root.join(request.strip_prefix("stdlib").unwrap_or(request));
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        Error::new(
            0,
            format!("could not open stash {}: {error}", candidate.display()),
        )
    })?;
    if !canonical.starts_with(&canonical_root) {
        return Err(Error::new(
            0,
            format!(
                "stash path {:?} escapes the bundled stdlib",
                request.display()
            ),
        ));
    }
    Ok(canonical)
}

fn integer(value: &str, path: &Path, line: usize, minimum: usize, maximum: usize) -> Result<usize> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| Error::new(line, "expected an integer").with_source(path))?;
    if !(minimum..=maximum).contains(&parsed) {
        return Err(Error::new(
            line,
            format!("value must be between {minimum} and {maximum}"),
        )
        .with_source(path));
    }
    Ok(parsed)
}

fn boolean(value: &str, path: &Path, line: usize) -> Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::new(line, "expected true or false").with_source(path)),
    }
}

fn quoted<'a>(value: &'a str, path: &Path, line: usize) -> Result<&'a str> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| Error::new(line, "expected a quoted path").with_source(path))
}

fn string_array(value: &str, path: &Path, line: usize) -> Result<Vec<String>> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| Error::new(line, "expected an array of quoted strings").with_source(path))?;
    let mut values = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        values.push(quoted(item, path, line)?.to_owned());
    }
    Ok(values)
}

fn without_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quoted {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character == '#' && !quoted {
            return &line[..index];
        }
    }
    line
}

fn test_profile_name(section: &str) -> Option<&str> {
    let name = section.strip_prefix("test.profiles.")?;
    valid_alias(name).then_some(name)
}

fn valid_alias(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn discovers_settings_and_resolves_a_linked_stash() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("isen-project-{unique}"));
        let app = base.join("app");
        let shared = base.join("shared");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            shared.join("math.is"),
            "dec answer @@ int = 42\nshare answer\n",
        )
        .unwrap();
        fs::write(
            app.join("isen.toml"),
            "[format]\nindent_width = 4\nmax_blank_lines = 0\nfinal_newline = false\n\n[test]\ndefault_profile = \"fast\"\n\n[test.profiles.fast]\npaths = [\"main.is\"]\nfail_fast = true\n\n[stash_links]\nlinked = \"../shared\"\n",
        )
        .unwrap();
        let entry = app.join("main.is");
        fs::write(&entry, "borrow answer from \"linked/math.is\"\n").unwrap();

        let config = ProjectConfig::discover(&entry).unwrap();
        assert_eq!(config.indent_width, 4);
        assert_eq!(config.max_blank_lines, 0);
        assert!(!config.final_newline);
        let (name, profile) = config.test_profile(None).unwrap().unwrap();
        assert_eq!(name, "fast");
        assert_eq!(
            profile.paths,
            vec![fs::canonicalize(app.join("main.is")).unwrap()]
        );
        assert!(profile.fail_fast);
        assert_eq!(
            resolve_stash(&entry, &entry, "linked/math.is").unwrap(),
            fs::canonicalize(shared.join("math.is")).unwrap()
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn resolves_bundled_stdlib_without_using_the_programme_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("isen-bundled-stdlib-{unique}"));
        let stdlib = base.join("distribution/stdlib");
        fs::create_dir_all(stdlib.join("logging")).unwrap();
        fs::write(
            stdlib.join("logging/human.is"),
            "dec answer = 42\nshare answer\n",
        )
        .unwrap();

        assert_eq!(
            resolve_bundled_stdlib(&stdlib, Path::new("stdlib/logging/human.is")).unwrap(),
            fs::canonicalize(stdlib.join("logging/human.is")).unwrap()
        );
        fs::remove_dir_all(base).unwrap();
    }
}
