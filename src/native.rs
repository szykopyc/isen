//! Stable-ish internal API for optional Rust-backed Isen spaces.
//!
//! Add a `.rs` file under `src/extensions/` containing a `register` function.
//! The build script discovers it automatically; no central enum or match arm is
//! required.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Data, EnvRef, Error, Expr, Result, Ty, Value, display, val};

thread_local! {
    static RANDOM_STATE: RefCell<Option<u64>> = const { RefCell::new(None) };
    static PROGRAM_ARGUMENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PROGRAM_KEYWORDS: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}

pub(crate) fn next_random_u64() -> u64 {
    RANDOM_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let current = state.get_or_insert_with(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|time| time.as_nanos() as u64)
                .unwrap_or(0xD10D_5C71_9A11_u64)
        });
        *current = current
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *current
    })
}

pub(crate) fn seed_random(seed: i64) {
    RANDOM_STATE.with(|state| *state.borrow_mut() = Some(seed as u64));
}

pub(crate) fn set_program_arguments(arguments: Vec<String>) -> std::result::Result<(), String> {
    let mut positional = Vec::new();
    let mut keywords = BTreeMap::new();
    for argument in arguments {
        let Some(keyword) = argument.strip_prefix("--") else {
            positional.push(argument);
            continue;
        };
        if keyword.is_empty() {
            positional.push(argument);
            continue;
        }
        let (name, value) = keyword.split_once('=').unwrap_or((keyword, "true"));
        if name.is_empty()
            || !name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
            })
        {
            return Err(format!("invalid keyword argument name {name:?}"));
        }
        if keywords.insert(name.into(), value.into()).is_some() {
            return Err(format!("duplicate keyword argument '--{name}'"));
        }
    }
    PROGRAM_ARGUMENTS.with(|current| *current.borrow_mut() = positional);
    PROGRAM_KEYWORDS.with(|current| *current.borrow_mut() = keywords);
    Ok(())
}

pub(crate) fn program_arguments() -> Vec<String> {
    PROGRAM_ARGUMENTS.with(|arguments| arguments.borrow().clone())
}

pub(crate) fn program_keywords() -> BTreeMap<String, String> {
    PROGRAM_KEYWORDS.with(|arguments| arguments.borrow().clone())
}

pub(crate) type NativeCallback = fn(NativeCall<'_>) -> Result<Value>;
pub(crate) type NativeRuntimeCallback = fn(&[Expr], EnvRef, usize) -> Result<Value>;
pub(crate) type NativeSignatures = fn() -> Vec<NativeSignature>;
pub(crate) type NativeRegister = fn(&mut NativeRegistry);
pub(crate) type NativeCleanup = fn();

#[derive(Clone)]
pub(crate) enum NativeExpected {
    Exact(Ty),
    Any,
    Number,
    Ordered,
    List,
    Map,
    SameAs(usize),
}

#[derive(Clone)]
pub(crate) enum NativeProduced {
    Exact(Ty),
    SameAs(usize),
    OptionalListElement(usize),
    OptionalMapValue(usize),
    MapKeys(usize),
    ArrayOfArgument(usize),
}

#[derive(Clone)]
pub(crate) struct NativeSignature {
    pub name: &'static str,
    pub parameters: Vec<NativeExpected>,
    pub result: NativeProduced,
}

impl NativeSignature {
    pub(crate) fn exact(name: &'static str, parameters: Vec<Ty>, result: Ty) -> Self {
        Self {
            name,
            parameters: parameters.into_iter().map(NativeExpected::Exact).collect(),
            result: NativeProduced::Exact(result),
        }
    }

    pub(crate) fn custom(
        name: &'static str,
        parameters: Vec<NativeExpected>,
        result: NativeProduced,
    ) -> Self {
        Self {
            name,
            parameters,
            result,
        }
    }
}

pub(crate) struct NativeFunction {
    pub name: &'static str,
    pub call: NativeCallback,
}

pub(crate) struct NativeSpace {
    pub name: &'static str,
    pub functions: &'static [NativeFunction],
    pub signatures: NativeSignatures,
}

#[derive(Clone, Copy)]
#[allow(dead_code)] // Extension-facing variants need not all be used by shipped spaces.
pub(crate) enum NativeConstant {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(&'static str),
}

pub(crate) struct NativeRuntimeFunction {
    pub name: &'static str,
    pub call: NativeRuntimeCallback,
}

pub(crate) struct NativeRuntimeSpace {
    pub name: &'static str,
    pub functions: &'static [NativeRuntimeFunction],
    pub signatures: NativeSignatures,
}

#[derive(Clone)]
pub(crate) enum NativePackage {
    Ordinary {
        name: &'static str,
        functions: &'static [NativeFunction],
        constants: Vec<(&'static str, NativeConstant)>,
    },
    Runtime {
        name: &'static str,
        functions: &'static [NativeRuntimeFunction],
        constants: Vec<(&'static str, NativeConstant)>,
    },
}

#[derive(Clone)]
pub(crate) struct NativePackageMetadata {
    pub functions: Vec<NativeSignature>,
    pub constants: Vec<(&'static str, Ty)>,
}

pub(crate) struct NativeRegistry {
    root: Option<EnvRef>,
    metadata: BTreeMap<&'static str, NativePackageMetadata>,
}

impl NativeRegistry {
    pub fn new(root: EnvRef) -> Self {
        Self {
            root: Some(root),
            metadata: BTreeMap::new(),
        }
    }

    pub(crate) fn metadata_only() -> Self {
        Self {
            root: None,
            metadata: BTreeMap::new(),
        }
    }

    pub(crate) fn into_metadata(self) -> BTreeMap<&'static str, NativePackageMetadata> {
        self.metadata
    }

    pub(crate) fn add_cleanup(&mut self, cleanup: NativeCleanup) {
        let Some(root) = &self.root else {
            return;
        };
        let mut root = root.borrow_mut();
        if !root
            .native_cleanups
            .iter()
            .any(|registered| std::ptr::fn_addr_eq(*registered, cleanup))
        {
            root.native_cleanups.push(cleanup);
        }
    }

    pub fn add(&mut self, definition: NativeSpace) {
        let Some(root) = &self.root else {
            self.record(
                definition.name,
                definition.functions.iter().map(|function| function.name),
                (definition.signatures)(),
            );
            return;
        };
        let previous = root.borrow_mut().packages.insert(
            definition.name.into(),
            NativePackage::Ordinary {
                name: definition.name,
                functions: definition.functions,
                constants: Vec::new(),
            },
        );
        assert!(
            previous.is_none(),
            "duplicate native space '{}'",
            definition.name
        );
    }

    pub fn add_runtime(&mut self, definition: NativeRuntimeSpace) {
        let Some(root) = &self.root else {
            self.record(
                definition.name,
                definition.functions.iter().map(|function| function.name),
                (definition.signatures)(),
            );
            return;
        };
        let previous = root.borrow_mut().packages.insert(
            definition.name.into(),
            NativePackage::Runtime {
                name: definition.name,
                functions: definition.functions,
                constants: Vec::new(),
            },
        );
        assert!(
            previous.is_none(),
            "duplicate native space '{}'",
            definition.name
        );
    }

    pub fn add_constant(&mut self, space: &str, name: &'static str, constant: NativeConstant) {
        let ty = match constant {
            NativeConstant::Int(_) => Ty::Int,
            NativeConstant::Float(_) => Ty::Float,
            NativeConstant::Bool(_) => Ty::Bool,
            NativeConstant::String(_) => Ty::String,
        };
        let Some(root) = &self.root else {
            self.metadata
                .get_mut(space)
                .unwrap_or_else(|| {
                    panic!("native space '{space}' must be registered before its constants")
                })
                .constants
                .push((name, ty));
            return;
        };
        let mut root = root.borrow_mut();
        let package = root.packages.get_mut(space).unwrap_or_else(|| {
            panic!("native space '{space}' must be registered before its constants")
        });
        let constants = match package {
            NativePackage::Ordinary { constants, .. }
            | NativePackage::Runtime { constants, .. } => constants,
        };
        assert!(
            !constants.iter().any(|(existing, _)| *existing == name),
            "duplicate native name '{space}.{name}'"
        );
        constants.push((name, constant));
    }

    fn record(
        &mut self,
        space: &'static str,
        functions: impl Iterator<Item = &'static str>,
        signatures: Vec<NativeSignature>,
    ) {
        let function_names = functions.collect::<Vec<_>>();
        let signature_names = signatures
            .iter()
            .map(|signature| signature.name)
            .collect::<Vec<_>>();
        assert_eq!(
            function_names, signature_names,
            "runtime functions and signatures differ for native space '{space}'"
        );
        let previous = self.metadata.insert(
            space,
            NativePackageMetadata {
                functions: signatures,
                constants: Vec::new(),
            },
        );
        assert!(previous.is_none(), "duplicate native space '{space}'");
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn seeded_random_sequences_repeat() {
        seed_random(9_021);
        let first = [next_random_u64(), next_random_u64(), next_random_u64()];
        seed_random(9_021);
        let second = [next_random_u64(), next_random_u64(), next_random_u64()];
        assert_eq!(first, second);
    }

    #[test]
    fn duplicate_program_keywords_are_rejected() {
        let error =
            set_program_arguments(vec!["--mode=one".into(), "--mode=two".into()]).unwrap_err();
        assert!(error.contains("duplicate keyword"));
        set_program_arguments(Vec::new()).unwrap();
    }

    use crate::Ty;
    use std::collections::BTreeMap;

    /// Discovers every shipped extension through a metadata-only registry, so
    /// no runtime root scope is ever constructed.
    fn discover() -> BTreeMap<&'static str, NativePackageMetadata> {
        let mut registry = NativeRegistry::metadata_only();
        crate::extensions::register_all(&mut registry);
        registry.into_metadata()
    }

    fn signature<'a>(
        metadata: &'a BTreeMap<&'static str, NativePackageMetadata>,
        space: &str,
        name: &str,
    ) -> &'a NativeSignature {
        metadata
            .get(space)
            .unwrap_or_else(|| panic!("metadata is missing space '{space}'"))
            .functions
            .iter()
            .find(|signature| signature.name == name)
            .unwrap_or_else(|| panic!("metadata is missing signature '{space}.{name}'"))
    }

    #[test]
    fn metadata_only_discovery_sees_ordinary_functions() {
        let metadata = discover();
        // String.lower: string -> string.
        let lower = signature(&metadata, "String", "lower");
        assert_eq!(lower.parameters.len(), 1);
        assert!(matches!(
            &lower.parameters[0],
            NativeExpected::Exact(Ty::String)
        ));
        assert!(matches!(&lower.result, NativeProduced::Exact(Ty::String)));
        // Maths.floor: float -> int.
        let floor = signature(&metadata, "Maths", "floor");
        assert_eq!(floor.parameters.len(), 1);
        assert!(matches!(
            &floor.parameters[0],
            NativeExpected::Exact(Ty::Float)
        ));
        assert!(matches!(&floor.result, NativeProduced::Exact(Ty::Int)));
    }

    #[test]
    fn metadata_only_discovery_sees_polymorphic_functions() {
        let metadata = discover();
        // Maths.min: number, same-as-argument -> same-as-argument.
        let min = signature(&metadata, "Maths", "min");
        assert_eq!(min.parameters.len(), 2);
        assert!(matches!(&min.parameters[0], NativeExpected::Number));
        assert!(matches!(&min.parameters[1], NativeExpected::SameAs(0)));

        let ordering = signature(&metadata, "Ordering", "less");
        assert!(matches!(&ordering.parameters[0], NativeExpected::Ordered));
        assert!(matches!(&ordering.parameters[1], NativeExpected::SameAs(0)));
        assert!(matches!(&min.result, NativeProduced::SameAs(0)));
        // String.find: string, string -> perchance int.
        let find = signature(&metadata, "String", "find");
        assert!(matches!(
            &find.result,
            NativeProduced::Exact(Ty::Perchance(inner)) if **inner == Ty::Int
        ));
    }

    #[test]
    fn ml_metadata_matches_the_build_feature() {
        let metadata = discover();
        assert_eq!(metadata.contains_key("ML"), cfg!(feature = "ml-kernels"));
    }

    #[test]
    fn metadata_only_discovery_sees_runtime_functions() {
        let metadata = discover();
        // Array.float is registered through NativeRuntimeSpace.
        let float = signature(&metadata, "Array", "float");
        assert_eq!(float.parameters.len(), 2);
        assert!(matches!(
            &float.parameters[0],
            NativeExpected::Exact(Ty::Int)
        ));
        assert!(matches!(
            &float.parameters[1],
            NativeExpected::Exact(Ty::Float)
        ));
        assert!(matches!(
            &float.result,
            NativeProduced::Exact(Ty::Arr(inner)) if **inner == Ty::Float
        ));
    }

    #[test]
    fn metadata_only_discovery_sees_constants() {
        let metadata = discover();
        let maths = metadata
            .get("Maths")
            .expect("Maths space should be discoverable");
        let pi = maths
            .constants
            .iter()
            .find(|(name, _)| *name == "pi")
            .expect("Maths.pi constant should be discoverable");
        assert_eq!(pi.1.clone(), Ty::Float);
        // Runtime spaces expose no constants.
        let array = metadata
            .get("Array")
            .expect("Array space should be discoverable");
        assert!(array.constants.is_empty());
    }
}

pub(crate) struct NativeCall<'a> {
    arguments: &'a [Value],
    line: usize,
}

#[allow(dead_code)] // This is an authoring API; a particular build may use only part of it.
impl<'a> NativeCall<'a> {
    pub fn new(arguments: &'a [Value], line: usize) -> Self {
        Self { arguments, line }
    }

    pub fn exactly(&self, count: usize, signature: &str) -> Result<()> {
        if self.arguments.len() == count {
            Ok(())
        } else {
            Err(self.error(format!(
                "{signature} expects {count} argument{}",
                if count == 1 { "" } else { "s" }
            )))
        }
    }

    pub fn float(&self, index: usize, signature: &str) -> Result<f64> {
        match self.arguments.get(index).map(|value| &value.data) {
            Some(Data::Float(value)) => Ok(*value),
            _ => Err(self.error(format!("{signature} expects float argument {}", index + 1))),
        }
    }

    pub fn int(&self, index: usize, signature: &str) -> Result<i64> {
        match self.arguments.get(index).map(|value| &value.data) {
            Some(Data::Int(value)) => Ok(*value),
            _ => Err(self.error(format!("{signature} expects int argument {}", index + 1))),
        }
    }

    pub fn bool(&self, index: usize, signature: &str) -> Result<bool> {
        match self.arguments.get(index).map(|value| &value.data) {
            Some(Data::Bool(value)) => Ok(*value),
            _ => Err(self.error(format!("{signature} expects bool argument {}", index + 1))),
        }
    }

    pub fn string(&self, index: usize, signature: &str) -> Result<&str> {
        match self.arguments.get(index).map(|value| &value.data) {
            Some(Data::String(value)) => Ok(value),
            _ => Err(self.error(format!("{signature} expects string argument {}", index + 1))),
        }
    }

    pub fn json(&self, index: usize, signature: &str) -> Result<&serde_json::Value> {
        match self.arguments.get(index).map(|value| &value.data) {
            Some(Data::Json(value)) => Ok(value),
            _ => Err(self.error(format!("{signature} expects json argument {}", index + 1))),
        }
    }

    pub fn shown(&self, index: usize, signature: &str) -> Result<String> {
        self.arguments
            .get(index)
            .map(display)
            .ok_or_else(|| self.error(format!("{signature} is missing argument {}", index + 1)))
    }

    pub fn value(&self, index: usize, signature: &str) -> Result<&Value> {
        self.arguments
            .get(index)
            .ok_or_else(|| self.error(format!("{signature} is missing argument {}", index + 1)))
    }

    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::new(self.line, message)
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn float_value(&self, value: f64) -> Value {
        val(Ty::Float, Data::Float(value))
    }

    pub fn int_value(&self, value: i64) -> Value {
        val(Ty::Int, Data::Int(value))
    }

    pub fn bool_value(&self, value: bool) -> Value {
        val(Ty::Bool, Data::Bool(value))
    }

    pub fn string_value(&self, value: impl Into<String>) -> Value {
        val(Ty::String, Data::String(value.into()))
    }

    pub fn json_value(&self, value: serde_json::Value) -> Value {
        val(Ty::Json, Data::Json(value))
    }

    pub fn unit_value(&self) -> Value {
        val(Ty::Unit, Data::Unit)
    }
}
