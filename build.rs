use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/extensions");

    let mut files = fs::read_dir("src/extensions")
        .expect("src/extensions must exist")
        .map(|entry| entry.expect("could not read native extension").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    files.sort();

    let mut generated = String::new();
    let mut modules = Vec::new();
    for path in files {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("native extension filenames must be UTF-8");
        assert!(
            stem.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "native extension filenames may only contain letters, digits, and underscores"
        );
        let module = format!("extension_{stem}");
        let absolute = fs::canonicalize(&path).expect("could not resolve native extension");
        generated.push_str(&format!(
            "#[path = {absolute:?}]\npub(crate) mod {module};\n"
        ));
        modules.push(module);
        println!("cargo:rerun-if-changed={}", path.display());
    }

    generated
        .push_str("pub(crate) fn register_all(registry: &mut crate::native::NativeRegistry) {\n");
    for module in &modules {
        generated.push_str(&format!("    {module}::register(registry);\n"));
    }
    generated.push_str("}\n");
    generated.push_str(
        "pub(crate) fn runtime_loaders() -> &'static [crate::native::NativeRegister] {\n    &[\n",
    );
    for module in &modules {
        generated.push_str(&format!("        {module}::register,\n"));
    }
    generated.push_str("    ]\n}\n");

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is unavailable"))
        .join("isen_extensions.rs");
    fs::write(output, generated).expect("could not generate native extension registry");
}
