use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::{Error, Expr, Parser, Result, Stmt, Ty, accepts, common_type, lex, same};

#[derive(Clone)]
enum Symbol {
    Value(Ty),
    Callable(Callable),
    Namespace(HashMap<String, Symbol>),
    Form(Vec<(String, Ty)>),
    Problem(Vec<(String, Ty)>),
}

#[derive(Clone)]
struct Callable {
    parameters: Vec<Expected>,
    result: Produced,
    generics: HashSet<String>,
}

#[derive(Clone)]
enum Expected {
    Exact(Ty),
    Any,
    Number,
    Ordered,
    List,
    Map,
    SameAs(usize),
}

#[derive(Clone)]
enum Produced {
    Exact(Ty),
    SameAs(usize),
    OptionalListElement(usize),
    OptionalMapValue(usize),
    MapKeys(usize),
    ArrayOfArgument(usize),
}

type Interface = HashMap<String, Symbol>;

#[derive(Default)]
struct Modules {
    loaded: RefCell<HashMap<PathBuf, Interface>>,
    loading: RefCell<Vec<PathBuf>>,
    project_source: Option<PathBuf>,
}

struct Checker {
    scopes: Vec<Interface>,
    shares: Interface,
    source: Option<PathBuf>,
    modules: Rc<Modules>,
    returns: Option<Ty>,
    loops: usize,
    current_namespace: Option<String>,
    namespace_scope: Option<usize>,
    generics: HashSet<String>,
    narrowings: HashMap<String, Ty>,
    borrow_origins: HashMap<(usize, String), String>,
}

pub(crate) fn check(program: &[Stmt], source: Option<&Path>) -> Result<()> {
    let mut checker = Checker {
        scopes: vec![core_globals()],
        shares: HashMap::new(),
        source: source.map(Path::to_owned),
        modules: Rc::new(Modules {
            project_source: source.map(Path::to_owned),
            ..Modules::default()
        }),
        returns: None,
        loops: 0,
        current_namespace: None,
        namespace_scope: None,
        generics: HashSet::new(),
        narrowings: HashMap::new(),
        borrow_origins: HashMap::new(),
    };
    checker.block(program)
}

impl Checker {
    fn block(&mut self, statements: &[Stmt]) -> Result<()> {
        for statement in statements {
            self.statement(statement)?;
        }
        Ok(())
    }

    fn child(&self) -> Self {
        let mut scopes = self.scopes.clone();
        scopes.push(HashMap::new());
        Self {
            scopes,
            shares: self.shares.clone(),
            source: self.source.clone(),
            modules: self.modules.clone(),
            returns: self.returns.clone(),
            loops: self.loops,
            current_namespace: self.current_namespace.clone(),
            namespace_scope: self.namespace_scope,
            generics: self.generics.clone(),
            narrowings: self.narrowings.clone(),
            borrow_origins: self.borrow_origins.clone(),
        }
    }

    fn statement(&mut self, statement: &Stmt) -> Result<()> {
        match statement {
            Stmt::Let(name, annotation, expression, line) => {
                if let Some(annotation) = annotation {
                    self.validate_ty(annotation, *line)?;
                }
                let symbol = if let Some(expected) = annotation {
                    self.expression_expected(expression, expected)?;
                    Symbol::Value(expected.clone())
                } else {
                    self.expression(expression)?
                };
                self.define(name, symbol);
            }
            Stmt::Assign(target, operator, value, line) => {
                let expected = self.assignable(target)?;
                let actual = self.expression_expected(value, &expected)?;
                self.require_type(&expected, &actual, *line, "assignment")?;
                if let Some(operator) = operator {
                    self.binary_type(&expected, operator, &actual, *line)?;
                }
                if operator.is_none() {
                    if let Expr::Name(name, _) = target {
                        self.narrowings.insert(name.clone(), actual);
                    }
                }
            }
            Stmt::Show(expressions, _)
            | Stmt::Warn(expressions, _)
            | Stmt::Raise(expressions, _)
            | Stmt::QuietRaise(expressions, _) => {
                for expression in expressions {
                    self.expression(expression)?;
                }
            }
            Stmt::Attempt(body, recoveries, always, line) => {
                self.child().block(body)?;
                let mut caught = Vec::new();
                for (name, ty, body) in recoveries {
                    if !self.is_problem_type(ty) {
                        return Err(Error::new(
                            *line,
                            format!("recover expects a problem type, got {ty}"),
                        ));
                    }
                    if caught.iter().any(|previous: &Ty| {
                        same(previous, ty)
                            || matches!(previous, Ty::Named(name) if name == "Problem")
                    }) {
                        return Err(Error::new(*line, format!("unreachable recover for {ty}")));
                    }
                    caught.push(ty.clone());
                    let mut recovery = self.child();
                    recovery.define(name, Symbol::Value(ty.clone()));
                    recovery.block(body)?;
                }
                if let Some(body) = always {
                    self.child().block(body)?;
                }
            }
            Stmt::If(condition, yes, no, line) => {
                self.require_expression(condition, &Ty::Bool, *line, "if condition")?;
                let mut yes_checker = self.child();
                yes_checker.apply_condition_narrowing(condition, true);
                yes_checker.block(yes)?;
                let mut no_checker = self.child();
                no_checker.apply_condition_narrowing(condition, false);
                no_checker.block(no)?;
            }
            Stmt::While(condition, body, line) => {
                self.require_expression(condition, &Ty::Bool, *line, "aslongas condition")?;
                let mut child = self.child();
                child.apply_condition_narrowing(condition, true);
                child.loops += 1;
                child.block(body)?;
            }
            Stmt::For(name, values, body, line) => {
                let collection = self.value_expression(values)?;
                let item = match collection {
                    Ty::List(item) | Ty::Arr(item) => *item,
                    Ty::Map(key, _) => *key,
                    _ => return Err(Error::new(*line, "each expects a list, array, or map")),
                };
                let mut child = self.child();
                child.loops += 1;
                child.define(name, Symbol::Value(item));
                child.block(body)?;
            }
            Stmt::Fn(name, declared_generics, parameters, result, body, line) => {
                let mut generics = HashSet::new();
                for generic in declared_generics {
                    if !is_generic_name(generic) {
                        return Err(Error::new(
                            *line,
                            format!(
                                "generic parameter '{generic}' must use uppercase letters, digits, or underscores"
                            ),
                        ));
                    }
                    if matches!(
                        self.lookup(generic),
                        Some(Symbol::Form(_) | Symbol::Problem(_))
                    ) {
                        return Err(Error::new(
                            *line,
                            format!(
                                "generic parameter '{generic}' conflicts with an existing type"
                            ),
                        ));
                    }
                    if !generics.insert(generic.clone()) {
                        return Err(Error::new(
                            *line,
                            format!("duplicate generic parameter '{generic}'"),
                        ));
                    }
                }
                let previous_generics = std::mem::replace(&mut self.generics, generics.clone());
                for (_, ty) in parameters {
                    self.validate_ty(ty, *line)?;
                }
                self.validate_ty(result, *line)?;
                self.generics = previous_generics;
                let callable = Callable {
                    parameters: parameters
                        .iter()
                        .map(|(_, ty)| Expected::Exact(ty.clone()))
                        .collect(),
                    result: Produced::Exact(result.clone()),
                    generics: generics.clone(),
                };
                self.define(name, Symbol::Callable(callable));
                let mut function = self.child();
                function.returns = Some(result.clone());
                function.generics = generics;
                function.loops = 0;
                for (parameter, ty) in parameters {
                    function.define(parameter, Symbol::Value(ty.clone()));
                }
                function.block(body)?;
                if !same(result, &Ty::Unit) && !guarantees_exit(body) {
                    return Err(Error::new(
                        *line,
                        format!("function '{name}' can finish without ret {result}"),
                    ));
                }
            }
            Stmt::Return(expression, _) => {
                if let Some(expected) = &self.returns {
                    let expected = expected.clone();
                    self.expression_expected(expression, &expected)?;
                } else {
                    self.value_expression(expression)?;
                }
            }
            Stmt::Enough(line) | Stmt::Onwards(line) if self.loops == 0 => {
                return Err(Error::new(
                    *line,
                    "loop control can only be used inside a loop",
                ));
            }
            Stmt::Enough(_) | Stmt::Onwards(_) | Stmt::Exit(_) => {}
            Stmt::Borrow(name, Some(path), alias, line) => {
                let (symbol, canonical) = self.borrow_local(name, path, *line)?;
                let binding = alias.as_ref().unwrap_or(name);
                let scope = self.scopes.len() - 1;
                let origin = format!("{}#{name}", canonical.display());
                if self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.contains_key(binding))
                {
                    if self.borrow_origins.get(&(scope, binding.clone())) == Some(&origin) {
                        return Ok(());
                    }
                    return Err(Error::new(
                        *line,
                        format!("borrow name '{binding}' is already defined in this scope"),
                    ));
                }
                if alias.is_some() && matches!(symbol, Symbol::Form(_) | Symbol::Problem(_)) {
                    return Err(Error::new(
                        *line,
                        format!("cannot alias shared type '{name}'; borrow it by its shared name"),
                    ));
                }
                self.define(binding, symbol);
                self.borrow_origins.insert((scope, binding.clone()), origin);
            }
            Stmt::Borrow(name, None, _, line) => {
                let scope = self.scopes.len() - 1;
                let origin = format!("extension:{name}");
                if self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.contains_key(name))
                {
                    if self.borrow_origins.get(&(scope, name.clone())) == Some(&origin) {
                        return Ok(());
                    }
                    return Err(Error::new(
                        *line,
                        format!("borrow name '{name}' is already defined in this scope"),
                    ));
                }
                let symbol = extension_symbols().remove(name).ok_or_else(|| {
                    Error::new(*line, format!("no shipped runtime space named '{name}'"))
                })?;
                self.define(name, symbol);
                self.borrow_origins.insert((scope, name.clone()), origin);
            }
            Stmt::Share(name, line) => {
                let symbol = self.lookup(name).ok_or_else(|| {
                    Error::new(*line, format!("cannot share unknown name '{name}'"))
                })?;
                self.shares.insert(name.clone(), symbol);
            }
            Stmt::Namespace(name, body, _) => {
                let mut namespace = self.child();
                namespace.current_namespace = Some(name.clone());
                namespace.namespace_scope = Some(namespace.scopes.len() - 1);
                namespace.block(body)?;
                let values = namespace.scopes.pop().unwrap_or_default();
                self.define(name, Symbol::Namespace(values));
            }
            Stmt::Form(name, fields, line) => {
                if name == "Problem" {
                    return Err(Error::new(*line, "Problem is a reserved built-in type"));
                }
                self.define(name, Symbol::Form(fields.clone()));
                for (_, ty) in fields {
                    self.validate_ty(ty, *line)?;
                }
            }
            Stmt::Problem(name, fields, line) => {
                if name == "Problem" {
                    return Err(Error::new(*line, "Problem is a reserved built-in type"));
                }
                self.define(name, Symbol::Problem(fields.clone()));
                for (_, ty) in fields {
                    self.validate_ty(ty, *line)?;
                }
            }
            Stmt::Expr(expression) => {
                self.expression(expression)?;
            }
        }
        Ok(())
    }

    fn expression(&mut self, expression: &Expr) -> Result<Symbol> {
        let line = expression.line();
        Ok(match expression {
            Expr::Int(_, _) => Symbol::Value(Ty::Int),
            Expr::Float(_, _) => Symbol::Value(Ty::Float),
            Expr::Bool(_, _) => Symbol::Value(Ty::Bool),
            Expr::String(_, _) => Symbol::Value(Ty::String),
            Expr::Naught(_) => Symbol::Value(Ty::Naught),
            Expr::Name(name, _) => self
                .lookup(name)
                .ok_or_else(|| Error::new(line, format!("unknown name '{name}'")))?,
            Expr::Declare(name, annotation, value, _) => {
                if let Some(annotation) = annotation {
                    self.validate_ty(annotation, line)?;
                }
                let symbol = if let Some(expected) = annotation {
                    self.expression_expected(value, expected)?;
                    Symbol::Value(expected.clone())
                } else {
                    self.expression(value)?
                };
                self.define(name, symbol.clone());
                symbol
            }
            Expr::List(items, _) => Symbol::Value(Ty::List(Box::new(
                self.collection_type(items, line, "list")?,
            ))),
            Expr::Arr(items, _) => Symbol::Value(Ty::Arr(Box::new(
                self.collection_type(items, line, "array")?,
            ))),
            Expr::Map(items, _) => {
                let mut keys = Vec::new();
                let mut values = Vec::new();
                for (key, value) in items {
                    keys.push(key.clone());
                    values.push(value.clone());
                }
                let key = self.collection_type(&keys, line, "map keys")?;
                if !matches!(key, Ty::String | Ty::Int | Ty::Bool) {
                    return Err(Error::new(line, "map keys must be string, int, or bool"));
                }
                let value = self.collection_type(&values, line, "map values")?;
                Symbol::Value(Ty::Map(Box::new(key), Box::new(value)))
            }
            Expr::Form(name, fields, _) => {
                let expected = match self
                    .lookup(name)
                    .ok_or_else(|| Error::new(line, format!("unknown form '{name}'")))?
                {
                    Symbol::Form(fields) | Symbol::Problem(fields) => fields,
                    _ => return Err(Error::new(line, format!("'{name}' is not a form"))),
                };
                if fields.len() != expected.len() {
                    return Err(Error::new(
                        line,
                        format!("{name} has the wrong number of fields"),
                    ));
                }
                let mut seen = HashSet::new();
                for (field, expression) in fields {
                    let ty = expected
                        .iter()
                        .find(|(candidate, _)| candidate == field)
                        .map(|(_, ty)| ty)
                        .ok_or_else(|| Error::new(line, format!("unknown field '{field}'")))?;
                    if !seen.insert(field.clone()) {
                        return Err(Error::new(line, format!("duplicate field '{field}'")));
                    }
                    self.require_expression(expression, ty, line, field)?;
                }
                if let Some((missing, _)) = expected.iter().find(|(field, _)| !seen.contains(field))
                {
                    return Err(Error::new(line, format!("missing field '{missing}'")));
                }
                Symbol::Value(Ty::Named(name.clone()))
            }
            Expr::Unary(operator, value, _) => {
                let ty = self.value_expression(value)?;
                match (operator.as_str(), ty) {
                    ("!", Ty::Bool) => Symbol::Value(Ty::Bool),
                    ("~", Ty::Int) => Symbol::Value(Ty::Int),
                    ("-", Ty::Int) => Symbol::Value(Ty::Int),
                    ("-", Ty::Float) => Symbol::Value(Ty::Float),
                    _ => return Err(Error::new(line, format!("invalid unary '{operator}'"))),
                }
            }
            Expr::Binary(left, operator, right, _) => {
                let left = self.value_expression(left)?;
                let right = self.value_expression(right)?;
                let result = self.binary_type(&left, operator, &right, line)?;
                Symbol::Value(result)
            }
            Expr::Call(callee, arguments, _) => {
                let Symbol::Callable(callable) = self.expression(callee)? else {
                    return Err(Error::new(line, "value is not callable"));
                };
                if arguments.len() != callable.parameters.len() {
                    return Err(Error::new(
                        line,
                        format!(
                            "expected {} arguments, got {}",
                            callable.parameters.len(),
                            arguments.len()
                        ),
                    ));
                }
                let mut actual = Vec::with_capacity(arguments.len());
                let mut substitutions = HashMap::new();
                for (index, (argument, expected)) in
                    arguments.iter().zip(&callable.parameters).enumerate()
                {
                    let ty = match expected {
                        Expected::Exact(ty) if contains_generic(ty, &callable.generics) => {
                            self.value_expression(argument)?
                        }
                        Expected::Exact(ty) => self.expression_expected(argument, ty)?,
                        _ => self.value_expression(argument)?,
                    };
                    actual.push(ty);
                    if let Expected::Exact(exact) = expected {
                        if contains_generic(exact, &callable.generics) {
                            unify_generics(
                                exact,
                                &actual[index],
                                &callable.generics,
                                &mut substitutions,
                                line,
                            )?;
                        } else {
                            self.check_expected(expected, &actual, index, line)?;
                        }
                    } else {
                        self.check_expected(expected, &actual, index, line)?;
                    }
                }
                let result = produced(&callable.result, &actual, line)?;
                Symbol::Value(substitute_generics(&result, &substitutions))
            }
            Expr::Cast(value, target, _) => {
                self.validate_ty(target, line)?;
                let source = self.value_expression(value)?;
                let optional_unwrap =
                    matches!(&source, Ty::Perchance(inner) if same(inner, target));
                let scalar_source = matches!(source, Ty::Int | Ty::Float | Ty::Bool | Ty::String)
                    || matches!(&source, Ty::Perchance(inner) if matches!(inner.as_ref(), Ty::Int | Ty::Float | Ty::Bool | Ty::String));
                let scalar_target = matches!(target, Ty::Int | Ty::Float | Ty::Bool | Ty::String);
                if !optional_unwrap && (!scalar_source || !scalar_target) {
                    return Err(Error::new(
                        line,
                        format!("cannot pour {source} into {target}"),
                    ));
                }
                Symbol::Value(target.clone())
            }
            Expr::Index(container, index, _) => {
                let container = self.value_expression(container)?;
                let index = self.value_expression(index)?;
                let result = match container {
                    Ty::List(item) | Ty::Arr(item) if same(&index, &Ty::Int) => *item,
                    Ty::Map(key, value) if same(&index, &key) => *value,
                    Ty::String if same(&index, &Ty::Int) => Ty::String,
                    _ => return Err(Error::new(line, "invalid index types")),
                };
                Symbol::Value(result)
            }
            Expr::Field(value, field, _) => match self.expression(value)? {
                Symbol::Namespace(values) => values.get(field).cloned().ok_or_else(|| {
                    Error::new(line, format!("unknown namespace member '{field}'"))
                })?,
                Symbol::Value(Ty::Named(name)) => {
                    let fields = match self
                        .lookup(&name)
                        .ok_or_else(|| Error::new(line, format!("unknown form '{name}'")))?
                    {
                        Symbol::Form(fields) | Symbol::Problem(fields) => fields,
                        _ => return Err(Error::new(line, format!("unknown form '{name}'"))),
                    };
                    let ty = fields
                        .iter()
                        .find(|(name, _)| name == field)
                        .map(|(_, ty)| ty.clone())
                        .ok_or_else(|| Error::new(line, format!("unknown field '{field}'")))?;
                    Symbol::Value(ty)
                }
                Symbol::Value(Ty::UdpPacket) => match field.as_str() {
                    "host" => Symbol::Value(Ty::String),
                    "port" => Symbol::Value(Ty::Int),
                    "bytes" => Symbol::Value(Ty::Arr(Box::new(Ty::Int))),
                    "text" => Symbol::Value(Ty::Perchance(Box::new(Ty::String))),
                    _ => {
                        return Err(Error::new(
                            line,
                            format!("udp_packet has no field '{field}'"),
                        ));
                    }
                },
                Symbol::Value(Ty::HttpResponse) => match field.as_str() {
                    "status" => Symbol::Value(Ty::Int),
                    "reason" | "version" => Symbol::Value(Ty::String),
                    "headers" => Symbol::Value(Ty::Map(Box::new(Ty::String), Box::new(Ty::String))),
                    "body" => Symbol::Value(Ty::Arr(Box::new(Ty::Int))),
                    "text" => Symbol::Value(Ty::Perchance(Box::new(Ty::String))),
                    _ => {
                        return Err(Error::new(
                            line,
                            format!("http_response has no field '{field}'"),
                        ));
                    }
                },
                _ => {
                    return Err(Error::new(
                        line,
                        "field access expects a space, form, or structured runtime value",
                    ));
                }
            },
        })
    }

    fn collection_type(&mut self, expressions: &[Expr], line: usize, name: &str) -> Result<Ty> {
        let Some(first) = expressions.first() else {
            return Err(Error::new(
                line,
                format!("empty {name} has no inferable type"),
            ));
        };
        let mut ty = self.value_expression(first)?;
        for expression in &expressions[1..] {
            let next = self.value_expression(expression)?;
            ty = common_type(&ty, &next).ok_or_else(|| {
                if name == "list" {
                    Error::new(line, format!("list contains both {ty} and {next}"))
                } else {
                    Error::new(line, format!("mixed {name} types {ty} and {next}"))
                }
            })?;
        }
        Ok(ty)
    }

    fn binary_type(&self, left: &Ty, operator: &str, right: &Ty, line: usize) -> Result<Ty> {
        let result = match operator {
            "+" if same(left, &Ty::String) && same(right, &Ty::String) => Ty::String,
            "+" | "-" | "*" | "/" | "%"
                if same(left, right) && matches!(left, Ty::Int | Ty::Float) =>
            {
                left.clone()
            }
            "&" | "|" | "^" | "<<" | ">>" if same(left, &Ty::Int) && same(right, &Ty::Int) => {
                Ty::Int
            }
            "<" | "<=" | ">" | ">="
                if same(left, right) && matches!(left, Ty::Int | Ty::Float | Ty::String) =>
            {
                Ty::Bool
            }
            "==" | "!=" if self.compatible(left, right) && self.equality_allowed(left, right) => {
                Ty::Bool
            }
            "&&" | "||" if same(left, &Ty::Bool) && same(right, &Ty::Bool) => Ty::Bool,
            _ => {
                return Err(Error::new(
                    line,
                    format!("cannot use '{operator}' with {left} and {right}"),
                ));
            }
        };
        Ok(result)
    }

    fn require_expression(
        &mut self,
        expression: &Expr,
        expected: &Ty,
        line: usize,
        name: &str,
    ) -> Result<()> {
        let actual = self.expression_expected(expression, expected)?;
        self.require_type(expected, &actual, line, name)
    }

    fn expression_expected(&mut self, expression: &Expr, expected: &Ty) -> Result<Ty> {
        match (expression, expected) {
            (Expr::List(items, _), Ty::List(item_type)) => {
                for item in items {
                    self.expression_expected(item, item_type)?;
                }
                Ok(expected.clone())
            }
            (Expr::Arr(items, _), Ty::Arr(item_type)) => {
                for item in items {
                    self.expression_expected(item, item_type)?;
                }
                Ok(expected.clone())
            }
            (Expr::Map(items, _), Ty::Map(key_type, value_type)) => {
                for (key, value) in items {
                    self.expression_expected(key, key_type)?;
                    self.expression_expected(value, value_type)?;
                }
                Ok(expected.clone())
            }
            _ => {
                let actual = self.value_expression(expression)?;
                self.require_type(expected, &actual, expression.line(), "expression")?;
                Ok(actual)
            }
        }
    }

    fn value_expression(&mut self, expression: &Expr) -> Result<Ty> {
        value_type(&self.expression(expression)?)
            .ok_or_else(|| Error::new(expression.line(), "expression is not a value"))
    }

    fn assignable(&mut self, expression: &Expr) -> Result<Ty> {
        match expression {
            Expr::Name(name, line) => value_type(
                &self
                    .lookup_declared(name)
                    .ok_or_else(|| Error::new(*line, format!("unknown name '{name}'")))?,
            )
            .ok_or_else(|| Error::new(*line, "target is not assignable data")),
            Expr::Index(container, index, line) => {
                let container = self.value_expression(container)?;
                let index = self.value_expression(index)?;
                match container {
                    Ty::List(item) | Ty::Arr(item) if same(&index, &Ty::Int) => Ok(*item),
                    Ty::Map(key, value) if same(&index, &key) => Ok(*value),
                    Ty::List(_) | Ty::Arr(_) => {
                        Err(Error::new(*line, "list or array index must be int"))
                    }
                    Ty::Map(key, _) => Err(Error::new(
                        *line,
                        format!("map key expects {key}, got {index}"),
                    )),
                    _ => Err(Error::new(
                        *line,
                        "only lists, arrays, and maps have mutable slots",
                    )),
                }
            }
            Expr::Field(owner, field, line) => match self.value_expression(owner)? {
                Ty::Named(name) => match self.lookup(&name) {
                    Some(Symbol::Form(fields) | Symbol::Problem(fields)) => fields
                        .into_iter()
                        .find(|(candidate, _)| candidate == field)
                        .map(|(_, ty)| ty)
                        .ok_or_else(|| Error::new(*line, format!("unknown field '{field}'"))),
                    _ => Err(Error::new(*line, "only form fields can be assigned")),
                },
                _ => Err(Error::new(*line, "only form fields can be assigned")),
            },
            _ => Err(Error::new(expression.line(), "invalid assignment target")),
        }
    }

    fn define(&mut self, name: &str, symbol: Symbol) {
        self.narrowings.remove(name);
        self.borrow_origins
            .remove(&(self.scopes.len() - 1, name.to_owned()));
        self.scopes.last_mut().unwrap().insert(name.into(), symbol);
    }

    fn apply_condition_narrowing(&mut self, condition: &Expr, truthy: bool) {
        let Expr::Binary(left, operator, right, _) = condition else {
            return;
        };
        if operator != "==" && operator != "!=" {
            return;
        }
        let name = match (left.as_ref(), right.as_ref()) {
            (Expr::Name(name, _), Expr::Naught(_)) | (Expr::Naught(_), Expr::Name(name, _)) => name,
            _ => return,
        };
        let Some(Symbol::Value(Ty::Perchance(inner))) = self.lookup_declared(name) else {
            return;
        };
        let is_present = (operator == "!=") == truthy;
        self.narrowings
            .insert(name.clone(), if is_present { *inner } else { Ty::Naught });
    }

    fn is_problem_type(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Named(name) if matches!(self.lookup(name), Some(Symbol::Problem(_))))
    }

    fn type_accepts(&self, expected: &Ty, actual: &Ty) -> bool {
        if accepts(expected, actual) {
            return true;
        }
        match (expected, actual) {
            (Ty::Named(name), actual) if name == "Problem" => self.is_problem_type(actual),
            (Ty::Perchance(expected), actual) => self.type_accepts(expected, actual),
            (Ty::List(expected), Ty::List(actual)) | (Ty::Arr(expected), Ty::Arr(actual)) => {
                self.type_accepts(expected, actual)
            }
            (Ty::Map(expected_key, expected_value), Ty::Map(actual_key, actual_value)) => {
                same(expected_key, actual_key) && self.type_accepts(expected_value, actual_value)
            }
            _ => false,
        }
    }

    fn compatible(&self, left: &Ty, right: &Ty) -> bool {
        self.type_accepts(left, right) || self.type_accepts(right, left)
    }

    fn equality_allowed(&self, left: &Ty, right: &Ty) -> bool {
        if matches!(
            (left, right),
            (Ty::Naught, Ty::Perchance(_)) | (Ty::Perchance(_), Ty::Naught)
        ) {
            return true;
        }
        self.type_supports_equality(left, &mut HashSet::new())
            && self.type_supports_equality(right, &mut HashSet::new())
    }

    fn type_supports_equality(&self, ty: &Ty, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Ty::UdpSocket | Ty::UdpPacket | Ty::TcpListener | Ty::TcpStream | Ty::HttpResponse => {
                false
            }
            Ty::Perchance(inner) | Ty::List(inner) | Ty::Arr(inner) => {
                self.type_supports_equality(inner, visiting)
            }
            Ty::Map(key, value) => {
                self.type_supports_equality(key, visiting)
                    && self.type_supports_equality(value, visiting)
            }
            Ty::Named(name) => {
                if !visiting.insert(name.clone()) {
                    return true;
                }
                let supported = match self.lookup_declared(name) {
                    Some(Symbol::Form(fields) | Symbol::Problem(fields)) => fields
                        .iter()
                        .all(|(_, ty)| self.type_supports_equality(ty, visiting)),
                    _ => false,
                };
                visiting.remove(name);
                supported
            }
            _ => true,
        }
    }

    fn require_type(&self, expected: &Ty, actual: &Ty, line: usize, name: &str) -> Result<()> {
        if self.type_accepts(expected, actual) {
            Ok(())
        } else {
            Err(Error::new(
                line,
                format!("{name} expects {expected}, got {actual}"),
            ))
        }
    }

    fn check_expected(
        &self,
        expected: &Expected,
        actual: &[Ty],
        index: usize,
        line: usize,
    ) -> Result<()> {
        let valid = match expected {
            Expected::Exact(ty) => self.type_accepts(ty, &actual[index]),
            Expected::Any => true,
            Expected::Number => matches!(actual[index], Ty::Int | Ty::Float),
            Expected::Ordered => matches!(actual[index], Ty::Int | Ty::Float | Ty::String),
            Expected::List => matches!(actual[index], Ty::List(_)),
            Expected::Map => matches!(actual[index], Ty::Map(_, _)),
            Expected::SameAs(other) => same(&actual[index], &actual[*other]),
        };
        if valid {
            Ok(())
        } else {
            Err(Error::new(
                line,
                format!(
                    "argument {} has incompatible type {}",
                    index + 1,
                    actual[index]
                ),
            ))
        }
    }

    fn validate_ty(&self, ty: &Ty, line: usize) -> Result<()> {
        match ty {
            Ty::Named(name) => match self.lookup(name) {
                Some(Symbol::Form(_) | Symbol::Problem(_)) => Ok(()),
                _ if self.generics.contains(name) => Ok(()),
                _ => Err(Error::new(line, format!("unknown form type '{name}'"))),
            },
            Ty::Perchance(inner) | Ty::List(inner) | Ty::Arr(inner) => {
                self.validate_ty(inner, line)
            }
            Ty::Map(key, value) => {
                self.validate_ty(key, line)?;
                self.validate_ty(value, line)
            }
            _ => Ok(()),
        }
    }

    fn lookup(&self, name: &str) -> Option<Symbol> {
        if self.current_namespace.as_deref() == Some(name) {
            return Some(Symbol::Namespace(
                self.namespace_scope
                    .and_then(|index| self.scopes.get(index).cloned())
                    .unwrap_or_default(),
            ));
        }
        let symbol = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned());
        match (symbol, self.narrowings.get(name)) {
            (Some(Symbol::Value(_)), Some(narrowed)) => Some(Symbol::Value(narrowed.clone())),
            (symbol, _) => symbol,
        }
    }

    fn lookup_declared(&self, name: &str) -> Option<Symbol> {
        if self.current_namespace.as_deref() == Some(name) {
            return self.lookup(name);
        }
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn borrow_local(&mut self, name: &str, path: &str, line: usize) -> Result<(Symbol, PathBuf)> {
        let source = self.source.as_ref().ok_or_else(|| {
            Error::new(
                line,
                format!("borrow {name} from {path:?} requires a source file"),
            )
        })?;
        let project_source = self.modules.project_source.as_deref().unwrap_or(source);
        let canonical =
            crate::project::resolve_stash(project_source, source, path).map_err(|mut error| {
                if error.line == 0 {
                    error.line = line;
                }
                error
            })?;
        let interface = check_module(canonical.clone(), self.modules.clone())?;
        let symbol = interface.get(name).cloned().ok_or_else(|| {
            Error::new(
                line,
                format!("stash {} does not share '{name}'", canonical.display()),
            )
        })?;
        Ok((symbol, canonical))
    }
}

fn check_module(path: PathBuf, modules: Rc<Modules>) -> Result<Interface> {
    if let Some(interface) = modules.loaded.borrow().get(&path).cloned() {
        return Ok(interface);
    }
    if let Some(position) = modules
        .loading
        .borrow()
        .iter()
        .position(|item| item == &path)
    {
        let mut chain = modules.loading.borrow()[position..]
            .iter()
            .map(|item| item.display().to_string())
            .collect::<Vec<_>>();
        chain.push(path.display().to_string());
        return Err(Error::new(
            0,
            format!("circular stash borrowing: {}", chain.join(" -> ")),
        )
        .with_source(&path));
    }
    modules.loading.borrow_mut().push(path.clone());
    let result = (|| {
        let source = fs::read_to_string(&path)
            .map_err(|error| Error::new(0, error.to_string()).with_source(&path))?;
        let program = Parser::new(lex(&source).map_err(|error| error.with_source(&path))?)
            .program()
            .map_err(|error| error.with_source(&path))?;
        let mut checker = Checker {
            scopes: vec![core_globals()],
            shares: HashMap::new(),
            source: Some(path.clone()),
            modules: modules.clone(),
            returns: None,
            loops: 0,
            current_namespace: None,
            namespace_scope: None,
            generics: HashSet::new(),
            narrowings: HashMap::new(),
            borrow_origins: HashMap::new(),
        };
        checker
            .block(&program)
            .map_err(|error| error.with_source(&path))?;
        Ok(checker.shares)
    })();
    modules.loading.borrow_mut().pop();
    let interface = result?;
    modules.loaded.borrow_mut().insert(path, interface.clone());
    Ok(interface)
}

fn value_type(symbol: &Symbol) -> Option<Ty> {
    match symbol {
        Symbol::Value(ty) => Some(ty.clone()),
        _ => None,
    }
}

fn is_generic_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn contains_generic(ty: &Ty, generics: &HashSet<String>) -> bool {
    match ty {
        Ty::Named(name) => generics.contains(name),
        Ty::Perchance(inner) | Ty::List(inner) | Ty::Arr(inner) => {
            contains_generic(inner, generics)
        }
        Ty::Map(key, value) => contains_generic(key, generics) || contains_generic(value, generics),
        _ => false,
    }
}

fn unify_generics(
    expected: &Ty,
    actual: &Ty,
    generics: &HashSet<String>,
    substitutions: &mut HashMap<String, Ty>,
    line: usize,
) -> Result<()> {
    match (expected, actual) {
        (Ty::Named(name), actual) if generics.contains(name) => {
            if let Some(previous) = substitutions.get(name) {
                if !same(previous, actual) {
                    return Err(Error::new(
                        line,
                        format!("generic {name} was {previous}, then {actual}"),
                    ));
                }
            } else {
                substitutions.insert(name.clone(), actual.clone());
            }
            Ok(())
        }
        (Ty::List(expected), Ty::List(actual)) | (Ty::Arr(expected), Ty::Arr(actual)) => {
            unify_generics(expected, actual, generics, substitutions, line)
        }
        (Ty::Map(expected_key, expected_value), Ty::Map(actual_key, actual_value)) => {
            unify_generics(expected_key, actual_key, generics, substitutions, line)?;
            unify_generics(expected_value, actual_value, generics, substitutions, line)
        }
        (Ty::Perchance(expected), Ty::Perchance(actual)) => {
            unify_generics(expected, actual, generics, substitutions, line)
        }
        (Ty::Perchance(expected), actual) => {
            unify_generics(expected, actual, generics, substitutions, line)
        }
        _ if crate::accepts(expected, actual) => Ok(()),
        _ => Err(Error::new(
            line,
            format!("expected {expected}, got {actual}"),
        )),
    }
}

fn substitute_generics(ty: &Ty, substitutions: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::Named(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Ty::Perchance(inner) => Ty::Perchance(Box::new(substitute_generics(inner, substitutions))),
        Ty::List(inner) => Ty::List(Box::new(substitute_generics(inner, substitutions))),
        Ty::Arr(inner) => Ty::Arr(Box::new(substitute_generics(inner, substitutions))),
        Ty::Map(key, value) => Ty::Map(
            Box::new(substitute_generics(key, substitutions)),
            Box::new(substitute_generics(value, substitutions)),
        ),
        _ => ty.clone(),
    }
}

fn produced(produced: &Produced, actual: &[Ty], line: usize) -> Result<Ty> {
    match produced {
        Produced::Exact(ty) => Ok(ty.clone()),
        Produced::SameAs(index) => Ok(actual[*index].clone()),
        Produced::OptionalListElement(index) => match &actual[*index] {
            Ty::List(item) => Ok(Ty::Perchance(item.clone())),
            _ => Err(Error::new(line, "list argument is required")),
        },
        Produced::OptionalMapValue(index) => match &actual[*index] {
            Ty::Map(_, value) => Ok(Ty::Perchance(value.clone())),
            _ => Err(Error::new(line, "map argument is required")),
        },
        Produced::MapKeys(index) => match &actual[*index] {
            Ty::Map(key, _) => Ok(Ty::List(key.clone())),
            _ => Err(Error::new(line, "map argument is required")),
        },
        Produced::ArrayOfArgument(index) => Ok(Ty::Arr(Box::new(actual[*index].clone()))),
    }
}

fn guarantees_exit(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Return(_, _) | Stmt::Raise(_, _) | Stmt::QuietRaise(_, _) | Stmt::Exit(_) => true,
        Stmt::If(_, yes, no, _) => !no.is_empty() && guarantees_exit(yes) && guarantees_exit(no),
        Stmt::Attempt(body, recoveries, always, _) => {
            always.as_ref().is_some_and(|body| guarantees_exit(body))
                || (guarantees_exit(body)
                    && recoveries.iter().all(|(_, _, body)| guarantees_exit(body)))
        }
        _ => false,
    })
}

fn callable(parameters: Vec<Expected>, result: Ty) -> Symbol {
    Symbol::Callable(Callable {
        parameters,
        result: Produced::Exact(result),
        generics: HashSet::new(),
    })
}
fn extension_symbols() -> Interface {
    let mut registry = crate::native::NativeRegistry::metadata_only();
    crate::extensions::register_all(&mut registry);
    registry
        .into_metadata()
        .into_iter()
        .map(|(space, metadata)| {
            let mut entries = metadata
                .functions
                .into_iter()
                .map(|signature| {
                    let parameters = signature
                        .parameters
                        .into_iter()
                        .map(native_expected)
                        .collect();
                    let result = native_produced(signature.result);
                    (
                        signature.name.into(),
                        Symbol::Callable(Callable {
                            parameters,
                            result,
                            generics: HashSet::new(),
                        }),
                    )
                })
                .collect::<Interface>();
            for (name, ty) in metadata.constants {
                entries.insert(name.into(), Symbol::Value(ty));
            }
            (space.into(), Symbol::Namespace(entries))
        })
        .collect()
}

fn native_expected(expected: crate::native::NativeExpected) -> Expected {
    match expected {
        crate::native::NativeExpected::Exact(ty) => Expected::Exact(ty),
        crate::native::NativeExpected::Any => Expected::Any,
        crate::native::NativeExpected::Number => Expected::Number,
        crate::native::NativeExpected::Ordered => Expected::Ordered,
        crate::native::NativeExpected::List => Expected::List,
        crate::native::NativeExpected::Map => Expected::Map,
        crate::native::NativeExpected::SameAs(index) => Expected::SameAs(index),
    }
}

fn native_produced(produced: crate::native::NativeProduced) -> Produced {
    match produced {
        crate::native::NativeProduced::Exact(ty) => Produced::Exact(ty),
        crate::native::NativeProduced::SameAs(index) => Produced::SameAs(index),
        crate::native::NativeProduced::OptionalListElement(index) => {
            Produced::OptionalListElement(index)
        }
        crate::native::NativeProduced::OptionalMapValue(index) => Produced::OptionalMapValue(index),
        crate::native::NativeProduced::MapKeys(index) => Produced::MapKeys(index),
        crate::native::NativeProduced::ArrayOfArgument(index) => Produced::ArrayOfArgument(index),
    }
}

fn core_globals() -> Interface {
    let mut globals = HashMap::new();
    globals.insert(
        "Problem".into(),
        Symbol::Problem(vec![("message".into(), Ty::String)]),
    );
    globals.insert("size".into(), callable(vec![Expected::Any], Ty::Int));
    globals.insert("unit".into(), Symbol::Value(Ty::Unit));
    globals
}
