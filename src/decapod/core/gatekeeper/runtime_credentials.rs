//! Bounded credential provenance for a crate-root Rust source file.
//!
//! This accepts a deliberately complete generator grammar and immutable local
//! Option/tuple flow. It never trusts a helper's name, an unknown method, or a
//! replacement field by itself. Unsupported values join to Unknown.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Expr, Lit, Pat, Stmt, Token};

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Unknown,
    Diverged,
    Runtime,
    Tuple(Vec<Value>),
    Option {
        none: bool,
        some: Option<Box<Value>>,
    },
}

type Bindings = BTreeMap<String, Value>;

pub(super) fn fields(source: &str) -> Vec<Range<usize>> {
    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    if !standard_namespace_is_unambiguous(&file) {
        return Vec::new();
    }
    let mut functions = BTreeMap::new();
    for item in &file.items {
        if let syn::Item::Fn(function) = item {
            let name = function.sig.ident.to_string();
            if functions.insert(name, function).is_some() {
                return Vec::new();
            }
        }
    }
    let generators = functions
        .iter()
        .filter_map(|(name, function)| generator(function).then_some(name.clone()))
        .collect();
    let mut analysis = Analysis {
        source,
        generators,
        fields: Vec::new(),
    };
    for function in functions.values() {
        if function.attrs.iter().all(inert) && function.sig.unsafety.is_none() {
            analysis.block(&function.block, &mut Bindings::new());
        }
    }
    analysis.fields
}

pub(super) fn inert(attribute: &syn::Attribute) -> bool {
    [
        "cfg", "test", "allow", "warn", "deny", "forbid", "doc", "inline", "cold", "must_use",
    ]
    .iter()
    .any(|name| attribute.path().is_ident(name))
}

pub(super) fn standard_namespace_is_unambiguous(file: &syn::File) -> bool {
    if file.attrs.iter().any(|attribute| !inert(attribute)) {
        return false;
    }
    let mut namespace = NamespaceHazards(false);
    namespace.visit_file(file);
    !namespace.0
}

struct NamespaceHazards(bool);
impl<'ast> Visit<'ast> for NamespaceHazards {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.ident == "std" || item.ident == "r#std" {
            self.0 = true;
        }
        syn::visit::visit_item_mod(self, item);
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let name = item.rename.as_ref().map_or(&item.ident, |(_, name)| name);
        if name == "std" || name == "r#std" {
            self.0 = true;
        }
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        fn aliases_std(tree: &syn::UseTree) -> bool {
            match tree {
                syn::UseTree::Path(path) => aliases_std(&path.tree),
                syn::UseTree::Rename(rename) => rename.rename == "std" || rename.rename == "r#std",
                syn::UseTree::Name(name) => name.ident == "std" || name.ident == "r#std",
                syn::UseTree::Group(group) => group.items.iter().any(aliases_std),
                // The caller establishes a modern crate edition. Absolute
                // ::std resolves through the extern prelude, never this glob.
                syn::UseTree::Glob(_) => false,
            }
        }
        if aliases_std(&item.tree) {
            self.0 = true;
        }
    }
    fn visit_item_macro(&mut self, _: &'ast syn::ItemMacro) {
        // A crate-level item macro may replace names or emit candidate items.
        self.0 = true;
    }
}

fn path_is(path: &syn::Path, names: &[&str], absolute: bool) -> bool {
    path.leading_colon.is_some() == absolute
        && path.segments.len() == names.len()
        && path
            .segments
            .iter()
            .zip(names)
            .all(|(segment, name)| segment.ident == *name && segment.arguments.is_empty())
}

fn expr_path(expression: &Expr, names: &[&str], absolute: bool) -> bool {
    matches!(expression, Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() && path_is(&path.path, names, absolute))
}

fn name(expression: &Expr) -> Option<String> {
    let Expr::Path(path) = expression else {
        return None;
    };
    if !path.attrs.is_empty() || path.qself.is_some() {
        return None;
    }
    Some(path.path.get_ident()?.to_string())
}

fn local(statement: &Stmt, mutable: bool) -> Option<&Expr> {
    let Stmt::Local(local) = statement else {
        return None;
    };
    if !local.attrs.is_empty() {
        return None;
    }
    let Pat::Ident(binding) = &local.pat else {
        return None;
    };
    if !binding.attrs.is_empty()
        || binding.by_ref.is_some()
        || binding.subpat.is_some()
        || binding.mutability.is_some() != mutable
    {
        return None;
    }
    let init = local.init.as_ref()?;
    if init.diverge.is_some() {
        return None;
    }
    Some(&init.expr)
}

fn local_name(statement: &Stmt) -> Option<String> {
    let Stmt::Local(local) = statement else {
        return None;
    };
    let Pat::Ident(binding) = &local.pat else {
        return None;
    };
    Some(binding.ident.to_string())
}

fn call<'a>(
    expression: &'a Expr,
    names: &[&str],
    absolute: bool,
    count: usize,
) -> Option<&'a Punctuated<Expr, Token![,]>> {
    let Expr::Call(call) = expression else {
        return None;
    };
    (call.attrs.is_empty() && call.args.len() == count && expr_path(&call.func, names, absolute))
        .then_some(&call.args)
}

fn extracted(expression: &Expr) -> Option<&Expr> {
    let Expr::Try(extraction) = expression else {
        return None;
    };
    if !extraction.attrs.is_empty() {
        return None;
    }
    let inner = extraction.expr.as_ref();
    if let Expr::MethodCall(method) = inner
        && method.method == "map_err"
        && method.attrs.is_empty()
        && method.turbofish.is_none()
        && method.args.len() == 1
        && matches!(&method.args[0], Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none())
    {
        return Some(&method.receiver);
    }
    Some(inner)
}

fn mutable_reference(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Reference(reference) if reference.attrs.is_empty() && reference.mutability.is_some() && name(&reference.expr).as_deref() == Some(expected))
}

fn method<'a>(
    expression: &'a Expr,
    receiver: &str,
    method_name: &str,
    count: usize,
) -> Option<&'a Punctuated<Expr, Token![,]>> {
    let Expr::MethodCall(method) = expression else {
        return None;
    };
    (method.attrs.is_empty()
        && method.method == method_name
        && method.turbofish.is_none()
        && method.args.len() == count
        && name(&method.receiver).as_deref() == Some(receiver))
    .then_some(&method.args)
}

fn generator(function: &syn::ItemFn) -> bool {
    if !function.attrs.is_empty()
        || function.sig.unsafety.is_some()
        || function.sig.asyncness.is_some()
        || !function.sig.inputs.is_empty()
        || !function.sig.generics.params.is_empty()
    {
        return false;
    }
    let statements = &function.block.stmts;
    if statements.len() != 6 {
        return false;
    }
    let Some(buffer_expr) = local(&statements[0], true) else {
        return false;
    };
    let Some(buffer) = local_name(&statements[0]) else {
        return false;
    };
    let Expr::Macro(buffer_macro) = buffer_expr else {
        return false;
    };
    if !buffer_macro.attrs.is_empty() || !path_is(&buffer_macro.mac.path, &["std", "vec"], true) {
        return false;
    }
    struct Repeat {
        element: Expr,
        _semi: Token![;],
        count: syn::LitInt,
    }
    impl syn::parse::Parse for Repeat {
        fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
            Ok(Self {
                element: input.parse()?,
                _semi: input.parse()?,
                count: input.parse()?,
            })
        }
    }
    let Ok(repeat) = syn::parse2::<Repeat>(buffer_macro.mac.tokens.clone()) else {
        return false;
    };
    if !matches!(&repeat.element, Expr::Lit(literal) if literal.attrs.is_empty() && matches!(&literal.lit, Lit::Int(value) if value.base10_parse::<u8>().ok() == Some(0) && value.suffix() == "u8"))
        || !repeat
            .count
            .base10_parse::<usize>()
            .is_ok_and(|count| count > 0)
    {
        return false;
    }
    let Some(file_expr) = local(&statements[1], true) else {
        return false;
    };
    let Some(file_name) = local_name(&statements[1]) else {
        return false;
    };
    let Some(file_expr) = extracted(file_expr) else {
        return false;
    };
    let Some(open_args) = call(file_expr, &["std", "fs", "File", "open"], true, 1) else {
        return false;
    };
    if !matches!(&open_args[0], Expr::Lit(literal) if literal.attrs.is_empty() && matches!(&literal.lit, Lit::Str(value) if value.value() == "/dev/urandom"))
    {
        return false;
    }
    let Stmt::Expr(read, Some(_)) = &statements[2] else {
        return false;
    };
    let Some(read) = extracted(read) else {
        return false;
    };
    let Some(read_args) = call(read, &["std", "io", "Read", "read_exact"], true, 2) else {
        return false;
    };
    if !mutable_reference(&read_args[0], &file_name) || !mutable_reference(&read_args[1], &buffer) {
        return false;
    }
    let Some(output_expr) = local(&statements[3], true) else {
        return false;
    };
    let Some(output) = local_name(&statements[3]) else {
        return false;
    };
    let Some(capacity_args) = call(
        output_expr,
        &["std", "string", "String", "with_capacity"],
        true,
        1,
    ) else {
        return false;
    };
    let Expr::Binary(capacity) = &capacity_args[0] else {
        return false;
    };
    if !capacity.attrs.is_empty()
        || !matches!(capacity.op, syn::BinOp::Mul(_))
        || method(&capacity.left, &buffer, "len", 0).is_none()
        || !matches!(capacity.right.as_ref(), Expr::Lit(literal) if literal.attrs.is_empty() && matches!(&literal.lit, Lit::Int(value) if value.base10_parse::<usize>().ok() == Some(2)))
    {
        return false;
    }
    let Stmt::Expr(Expr::ForLoop(iteration), None) = &statements[4] else {
        return false;
    };
    let Pat::Ident(byte) = iteration.pat.as_ref() else {
        return false;
    };
    if !iteration.attrs.is_empty()
        || iteration.label.is_some()
        || !byte.attrs.is_empty()
        || byte.by_ref.is_some()
        || byte.mutability.is_some()
        || byte.subpat.is_some()
        || name(&iteration.expr).as_deref() != Some(&buffer)
        || iteration.body.stmts.len() != 1
    {
        return false;
    }
    let byte = byte.ident.to_string();
    let names = [&buffer, &file_name, &output, &byte];
    if names.iter().any(|name| name.starts_with("r#"))
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
    {
        return false;
    }
    let Stmt::Expr(push, Some(_)) = &iteration.body.stmts[0] else {
        return false;
    };
    let Some(push_args) = method(push, &output, "push_str", 1) else {
        return false;
    };
    let Expr::Reference(reference) = &push_args[0] else {
        return false;
    };
    let Expr::Macro(format) = reference.expr.as_ref() else {
        return false;
    };
    if !reference.attrs.is_empty()
        || reference.mutability.is_some()
        || !format.attrs.is_empty()
        || !path_is(&format.mac.path, &["std", "format"], true)
    {
        return false;
    }
    let Ok(format_literal) = syn::parse2::<syn::LitStr>(format.mac.tokens.clone()) else {
        return false;
    };
    if format_literal.value() != format!("{{{byte}:02x}}") {
        return false;
    }
    let Stmt::Expr(result, None) = &statements[5] else {
        return false;
    };
    call(result, &["std", "result", "Result", "Ok"], true, 1)
        .is_some_and(|args| name(&args[0]).as_deref() == Some(&output))
}

struct Analysis<'a> {
    source: &'a str,
    generators: BTreeSet<String>,
    fields: Vec<Range<usize>>,
}

impl Analysis<'_> {
    fn block(&mut self, block: &syn::Block, outer: &mut Bindings) -> Value {
        let mut bindings = outer.clone();
        let mut result = Value::Unknown;
        for statement in &block.stmts {
            result = Value::Unknown;
            match statement {
                Stmt::Local(local) => {
                    let value = local.init.as_ref().map_or(Value::Unknown, |init| {
                        if init.diverge.is_some() {
                            bindings.clear();
                            Value::Unknown
                        } else {
                            self.expression(&init.expr, &mut bindings)
                        }
                    });
                    if local.attrs.is_empty() {
                        bind(&local.pat, value, &mut bindings);
                    } else {
                        bindings.clear();
                    }
                }
                Stmt::Expr(expression, semi) => {
                    let value = self.expression(expression, &mut bindings);
                    if semi.is_none() || value == Value::Diverged {
                        result = value;
                    }
                }
                Stmt::Macro(invocation) => {
                    if invocation.attrs.is_empty() {
                        self.formatting(&invocation.mac, &mut bindings);
                    } else {
                        bindings.clear();
                    }
                }
                Stmt::Item(_) => {
                    // Local items can shadow names and contain expansion.
                    bindings.clear();
                }
            }
            if result == Value::Diverged {
                break;
            }
        }
        outer.retain(|name, value| bindings.get(name) == Some(value));
        result
    }

    fn expression(&mut self, expression: &Expr, bindings: &mut Bindings) -> Value {
        match expression {
            Expr::Path(path) if path.attrs.is_empty() && path.qself.is_none() => {
                if path_is(&path.path, &["std", "option", "Option", "None"], true) {
                    Value::Option {
                        none: true,
                        some: None,
                    }
                } else {
                    name(expression)
                        .and_then(|name| bindings.get(&name).cloned())
                        .unwrap_or(Value::Unknown)
                }
            }
            Expr::Paren(paren) if paren.attrs.is_empty() => self.expression(&paren.expr, bindings),
            Expr::Group(group) if group.attrs.is_empty() => self.expression(&group.expr, bindings),
            Expr::Tuple(tuple) if tuple.attrs.is_empty() => Value::Tuple(
                tuple
                    .elems
                    .iter()
                    .map(|expr| self.expression(expr, bindings))
                    .collect(),
            ),
            Expr::Call(call) if call.attrs.is_empty() => {
                if expr_path(&call.func, &["std", "option", "Option", "Some"], true)
                    && call.args.len() == 1
                {
                    return Value::Option {
                        none: false,
                        some: Some(Box::new(self.expression(&call.args[0], bindings))),
                    };
                }
                self.expression(&call.func, bindings);
                for argument in &call.args {
                    self.expression(argument, bindings);
                }
                Value::Unknown
            }
            Expr::Try(extraction) if extraction.attrs.is_empty() => {
                if let Expr::Call(call) = extraction.expr.as_ref()
                    && call.attrs.is_empty()
                    && call.args.is_empty()
                    && let Expr::Path(function) = call.func.as_ref()
                    && function.attrs.is_empty()
                    && function.qself.is_none()
                    && function.path.leading_colon.is_none()
                    && function.path.segments.len() == 2
                    && function.path.segments[0].ident == "crate"
                    && function
                        .path
                        .segments
                        .iter()
                        .all(|segment| segment.arguments.is_empty())
                    && self
                        .generators
                        .contains(&function.path.segments[1].ident.to_string())
                {
                    return Value::Runtime;
                }
                self.expression(&extraction.expr, bindings);
                Value::Unknown
            }
            Expr::Block(block) if block.attrs.is_empty() && block.label.is_none() => {
                self.block(&block.block, bindings)
            }
            Expr::If(branch) if branch.attrs.is_empty() => {
                let mut then_bindings = bindings.clone();
                let condition = self.expression(&branch.cond, &mut then_bindings);
                // Conditions execute before either branch. Preserve their
                // invalidations on the false path as well as the true path.
                bindings.retain(|name, value| then_bindings.get(name) == Some(value));
                if condition == Value::Diverged {
                    return Value::Diverged;
                }
                let mut else_bindings = bindings.clone();
                let yes = self.block(&branch.then_branch, &mut then_bindings);
                let no = branch
                    .else_branch
                    .as_ref()
                    .map_or(Value::Unknown, |(_, expr)| {
                        self.expression(expr, &mut else_bindings)
                    });
                bindings.retain(|name, value| {
                    (yes == Value::Diverged || then_bindings.get(name) == Some(value))
                        && (no == Value::Diverged || else_bindings.get(name) == Some(value))
                });
                join(yes, no)
            }
            Expr::Let(pattern) if pattern.attrs.is_empty() => {
                let value = self.expression(&pattern.expr, bindings);
                bind(&pattern.pat, value, bindings);
                Value::Unknown
            }
            Expr::Match(expression) if expression.attrs.is_empty() => {
                let value = self.expression(&expression.expr, bindings);
                let mut result = None;
                let mut continuing = Vec::new();
                let mut remaining = bindings.clone();
                for arm in &expression.arms {
                    let mut local = remaining.clone();
                    if !arm.attrs.iter().all(inert) {
                        result = Some(Value::Unknown);
                        continuing.push(Bindings::new());
                        continue;
                    }
                    if !bind(&arm.pat, value.clone(), &mut local) {
                        continue;
                    }
                    if let Some((_, guard)) = &arm.guard {
                        self.expression(guard, &mut local);
                        // A false guard can fall through to the next arm after
                        // executing side effects. Do not restore its origins.
                        remaining.retain(|name, value| local.get(name) == Some(value));
                    }
                    let arm_value = self.expression(&arm.body, &mut local);
                    if arm_value != Value::Diverged {
                        continuing.push(local);
                    }
                    result = Some(
                        result.map_or(arm_value.clone(), |previous| join(previous, arm_value)),
                    );
                }
                bindings.retain(|name, value| {
                    continuing
                        .iter()
                        .all(|local| local.get(name) == Some(value))
                });
                result.unwrap_or(Value::Unknown)
            }
            Expr::Return(returned) if returned.attrs.is_empty() => {
                if let Some(expression) = &returned.expr {
                    self.expression(expression, bindings);
                }
                Value::Diverged
            }
            Expr::Macro(invocation) if invocation.attrs.is_empty() => {
                self.formatting(&invocation.mac, bindings);
                Value::Unknown
            }
            Expr::Reference(reference) if reference.attrs.is_empty() => {
                if reference.mutability.is_some() {
                    bindings.clear();
                }
                self.expression(&reference.expr, bindings);
                Value::Unknown
            }
            Expr::MethodCall(method) if method.attrs.is_empty() => {
                // Unknown methods can mutate a receiver. Conservatively erase
                // its local origin, and evaluate all argument side effects.
                if let Some(receiver) = name(&method.receiver) {
                    bindings.remove(&receiver);
                }
                self.expression(&method.receiver, bindings);
                for argument in &method.args {
                    self.expression(argument, bindings);
                }
                Value::Unknown
            }
            Expr::Binary(binary) if binary.attrs.is_empty() => {
                if matches!(binary.op, syn::BinOp::And(_) | syn::BinOp::Or(_)) {
                    self.expression(&binary.left, bindings);
                    self.expression(&binary.right, bindings);
                } else {
                    bindings.clear();
                }
                Value::Unknown
            }
            Expr::Field(field) if field.attrs.is_empty() => {
                self.expression(&field.base, bindings);
                Value::Unknown
            }
            Expr::Struct(structure) if structure.attrs.is_empty() => {
                for field in &structure.fields {
                    if !field.attrs.is_empty() {
                        bindings.clear();
                    }
                    self.expression(&field.expr, bindings);
                }
                if let Some(rest) = &structure.rest {
                    self.expression(rest, bindings);
                }
                Value::Unknown
            }
            Expr::Lit(literal) if literal.attrs.is_empty() => Value::Unknown,
            // Unsupported scopes, mutation, unsafe/deferred execution, or
            // attributes cannot carry an earlier proof into later statements.
            _ => {
                bindings.clear();
                Value::Unknown
            }
        }
    }

    fn formatting(&mut self, invocation: &syn::Macro, bindings: &mut Bindings) {
        let standard = [
            "format",
            "format_args",
            "print",
            "println",
            "eprint",
            "eprintln",
        ]
        .iter()
        .any(|name| path_is(&invocation.path, &["std", name], true));
        if !standard {
            bindings.clear();
            return;
        }
        let Ok(arguments) =
            Punctuated::<Expr, Token![,]>::parse_terminated.parse2(invocation.tokens.clone())
        else {
            bindings.clear();
            return;
        };
        let mut hazards = super::rust_context::CaptureHazards(false);
        let mut attributes = super::rust_context::ArgumentAttributes(false);
        for argument in arguments.iter().skip(1) {
            attributes.visit_expr(argument);
            if let Expr::Assign(binding) = argument {
                hazards.visit_expr(&binding.right);
            } else {
                hazards.visit_expr(argument);
            }
        }
        if hazards.0 || attributes.0 {
            bindings.clear();
            return;
        }
        let mut explicit = BTreeMap::new();
        let mut positional = false;
        for argument in arguments.iter().skip(1) {
            let Expr::Assign(assignment) = argument else {
                self.expression(argument, bindings);
                positional = true;
                continue;
            };
            let Some(name) = name(&assignment.left) else {
                bindings.clear();
                return;
            };
            if !assignment.attrs.is_empty() || explicit.contains_key(&name) {
                bindings.clear();
                return;
            }
            let value = self.expression(&assignment.right, bindings);
            explicit.insert(name, value);
        }
        let Some(Expr::Lit(expression)) = arguments.first() else {
            bindings.clear();
            return;
        };
        if !expression.attrs.is_empty() {
            bindings.clear();
            return;
        }
        let Lit::Str(literal) = &expression.lit else {
            bindings.clear();
            return;
        };
        let span = literal.token().span().byte_range();
        let Some(original) = self.source.get(span.clone()) else {
            return;
        };
        if original != literal.token().to_string() {
            return;
        }
        let Some((body, offset)) = super::rust_context::literal_body(original) else {
            return;
        };
        let Some(fields) = super::rust_context::replacement_fields(body) else {
            return;
        };
        if positional
            || explicit
                .keys()
                .any(|name| !fields.iter().any(|(_, field)| *field == name))
        {
            return;
        }
        for (field, name) in fields {
            if matches!(
                explicit.get(name).or_else(|| bindings.get(name)),
                Some(Value::Runtime)
            ) {
                self.fields
                    .push(span.start + offset + field.start..span.start + offset + field.end);
            }
        }
    }
}

fn join(left: Value, right: Value) -> Value {
    match (left, right) {
        (Value::Diverged, value) | (value, Value::Diverged) => value,
        (Value::Runtime, Value::Runtime) => Value::Runtime,
        (Value::Tuple(left), Value::Tuple(right)) if left.len() == right.len() => Value::Tuple(
            left.into_iter()
                .zip(right)
                .map(|(left, right)| join(left, right))
                .collect(),
        ),
        (
            Value::Option {
                none: left_none,
                some: left,
            },
            Value::Option {
                none: right_none,
                some: right,
            },
        ) => Value::Option {
            none: left_none || right_none,
            some: match (left, right) {
                (Some(left), Some(right)) => Some(Box::new(join(*left, *right))),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
        },
        _ => Value::Unknown,
    }
}

// Returns false only when a fully known Option shape proves an arm unreachable.
fn bind(pattern: &Pat, value: Value, bindings: &mut Bindings) -> bool {
    let mut attributes = super::rust_context::ArgumentAttributes(false);
    attributes.visit_pat(pattern);
    if attributes.0 {
        bindings.clear();
        return true;
    }
    match pattern {
        Pat::Ident(binding)
            if binding.attrs.is_empty() && binding.by_ref.is_none() && binding.subpat.is_none() =>
        {
            bindings.insert(
                binding.ident.to_string(),
                if binding.mutability.is_none() {
                    value
                } else {
                    Value::Unknown
                },
            );
            true
        }
        Pat::Tuple(tuple) if tuple.attrs.is_empty() => {
            let values = match value {
                Value::Tuple(values) if values.len() == tuple.elems.len() => values,
                _ => vec![Value::Unknown; tuple.elems.len()],
            };
            for (pattern, value) in tuple.elems.iter().zip(values) {
                if !bind(pattern, value, bindings) {
                    return false;
                }
            }
            true
        }
        Pat::TupleStruct(tuple)
            if tuple.attrs.is_empty()
                && tuple.qself.is_none()
                && path_is(&tuple.path, &["std", "option", "Option", "Some"], true)
                && tuple.elems.len() == 1 =>
        {
            match value {
                Value::Option {
                    some: Some(value), ..
                } => bind(&tuple.elems[0], *value, bindings),
                Value::Option { some: None, .. } => false,
                _ => bind(&tuple.elems[0], Value::Unknown, bindings),
            }
        }
        Pat::Path(path)
            if path.attrs.is_empty()
                && path.qself.is_none()
                && path_is(&path.path, &["std", "option", "Option", "None"], true) =>
        {
            !matches!(value, Value::Option { none: false, .. })
        }
        Pat::Wild(wild) if wild.attrs.is_empty() => true,
        // Known syntax can only shadow the identifiers it actually binds.
        // An opaque pattern macro can introduce arbitrary names.
        _ => {
            struct UnknownBindings<'a>(&'a mut Bindings);
            impl<'ast> Visit<'ast> for UnknownBindings<'_> {
                fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
                    self.0.insert(pattern.ident.to_string(), Value::Unknown);
                    syn::visit::visit_pat_ident(self, pattern);
                }
                fn visit_macro(&mut self, _: &'ast syn::Macro) {
                    self.0.clear();
                }
            }
            UnknownBindings(bindings).visit_pat(pattern);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generator_source() -> &'static str {
        r#"fn generated() -> Result<String, Error> {
            let mut bytes = ::std::vec![0u8; 24];
            let mut file = ::std::fs::File::open("/dev/urandom").map_err(Error::Io)?;
            ::std::io::Read::read_exact(&mut file, &mut bytes).map_err(Error::Io)?;
            let mut encoded = ::std::string::String::with_capacity(bytes.len() * 2);
            for byte in bytes { encoded.push_str(&::std::format!("{byte:02x}")); }
            ::std::result::Result::Ok(encoded)
        }"#
    }

    fn sample(body: &str) -> String {
        format!("{}\nfn example() {{ {body} }}", generator_source())
    }

    #[test]
    fn generator_identity_and_immutable_captures_are_proven() {
        for body in [
            r#"let credential = crate::generated()?; ::std::println!("Password: {credential}");"#,
            r#"let issued = if existing { ::std::option::Option::Some(::std::option::Option::None) } else { let value = crate::generated()?; ::std::option::Option::Some(::std::option::Option::Some((record, value))) }; match issued { ::std::option::Option::Some(::std::option::Option::Some((record, credential))) => { ::std::println!("Record: {}", record.id); ::std::println!("Password: {credential}"); }, _ => {} }"#,
        ] {
            let source = sample(body);
            let result = fields(&source);
            assert_eq!(result.len(), 1, "missing proof: {body}");
            assert_eq!(&source[result[0].clone()], "{credential}");
        }
    }

    #[test]
    fn helper_names_or_partial_generator_bodies_do_not_prove_randomness() {
        for (from, to) in [
            ("/dev/urandom", "credential.txt"),
            ("::std::vec!", "vec!"),
            ("::std::format!", "format!"),
            (
                "::std::io::Read::read_exact(&mut file, &mut bytes)",
                "file.read_exact(&mut bytes)",
            ),
            ("0u8; 24", "0u8; 0"),
            (
                "::std::result::Result::Ok(encoded)",
                "encoded.push_str(\"literal-secret\"); ::std::result::Result::Ok(encoded)",
            ),
            (
                "::std::result::Result::Ok(encoded)",
                "::std::result::Result::Ok(\"literal-secret\".into())",
            ),
            ("let mut encoded", "let mut bytes"),
            ("fn generated", "#[rewrite] fn generated"),
        ] {
            let source = sample(r#"let credential = crate::generated()?; ::std::println!("Password: {credential}");"#).replace(from, to);
            assert!(
                fields(&source).is_empty(),
                "unproven generator: {from} -> {to}"
            );
        }
    }

    #[test]
    fn unknown_alternatives_and_mutation_taint_captures() {
        for body in [
            r#"let credential = generated()?; ::std::println!("Password: {credential}");"#,
            r#"let credential = if choose { crate::generated()? } else { "literal-secret".into() }; ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; let credential = "literal-secret"; ::std::println!("Password: {credential}");"#,
            r#"let mut credential = crate::generated()?; credential = "literal-secret".into(); ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; replace!(credential); ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; { unsafe { overwrite(&credential); } } ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; if condition { replace!(credential); } ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; if replace!(credential) { return; } ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; return; ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; match condition { true => { unsafe { overwrite(&credential); } }, false => {} } ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; match condition { true if change!(credential) => {}, _ => ::std::println!("Password: {credential}") }"#,
            r#"let credential = crate::generated()?; (unsafe { overwrite(&credential); callback })(); ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; let Some(result) = change!(credential) else { return }; ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; ::std::println!("{}", unsafe { overwrite(&credential) }); ::std::println!("Password: {credential}");"#,
            r#"let credential = crate::generated()?; ::std::println!("Password: {credential}", credential = "literal-secret");"#,
            r#"let credential = crate::generated()?; let f = |credential| ::std::println!("Password: {credential}");"#,
            r#"let issued = if choose { ::std::option::Option::Some(crate::generated()?) } else { unknown() }; match issued { ::std::option::Option::Some(credential) => ::std::println!("Password: {credential}"), _ => {} }"#,
        ] {
            assert!(fields(&sample(body)).is_empty(), "unknown flow: {body}");
        }
    }

    #[test]
    fn actual_session_acquisition_keeps_generator_to_match_proof() {
        let source = include_str!("../../lib.rs");
        let ranges = fields(source);
        let expected = source
            .find("Password: {password}")
            .expect("actual password formatter")
            + "Password: ".len();
        assert!(
            ranges.contains(&(expected..expected + "{password}".len())),
            "actual session source not proven: {ranges:?}"
        );
    }
}
