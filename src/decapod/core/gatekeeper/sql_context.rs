//! Bounded evidence for positional SQL parameters in Rust source.
//!
//! Literal positional binds require a terminal external Dactyl Connection
//! call, or an authenticated local wrapper forwarding directly to it. Operation
//! constructors alone are data and never establish a sink: their SQL can be
//! extracted and sent to a shell. A bounded local consumer may forward an owned
//! Operation parameter once to the terminal; observation/escape/opaque macro
//! expansion invalidates that summary. No returned-string flow is inferred.
//!
//! The external crates named `dactyl_db` and `async_trait` are assumed to be the
//! genuine dependencies; source analysis cannot authenticate Cargo downloads.
//! Production analysis validates a modern Cargo library's declared module
//! graph. Unknown syntax, ambiguous bindings and unavailable evidence decline.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use syn::visit::Visit;
use syn::{Expr, Item, Lit};

#[derive(Default)]
pub(super) struct SqlContext {
    parameters: Vec<ParameterSpan>,
}

struct ParameterSpan {
    token: Range<usize>,
    // The legacy dangerous-pattern regex also consumes one punctuation byte.
    punctuation_end: usize,
}

impl SqlContext {
    #[cfg(test)]
    pub(super) fn parse(source: &str) -> Self {
        Self::parse_with_wrappers(source, &BTreeSet::new())
    }

    pub(super) fn parse_file(repo_root: &Path, source_path: &Path, source: &str) -> Self {
        if !source
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'$' && pair[1].is_ascii_digit())
        {
            return Self::default();
        }
        let Some(crate_root) =
            owning_crate_root(repo_root, source_path, source.contains("async_trait"))
        else {
            return Self::default();
        };
        let wrappers = verified_wrappers(repo_root, &crate_root, source);
        Self::parse_with_wrappers(source, &wrappers)
    }

    fn parse_with_wrappers(source: &str, wrappers: &BTreeSet<Vec<String>>) -> Self {
        if source.starts_with('\u{feff}')
            || (source.starts_with("#!") && !source.starts_with("#!["))
        {
            return Self::default();
        }
        let Ok(file) = syn::parse_file(source) else {
            return Self::default();
        };
        let mut names = Names {
            check_attributes: true,
            ..Names::default()
        };
        for item in &file.items {
            if let Item::Use(item) = item
                && item.attrs.is_empty()
            {
                let mut imports = Vec::new();
                flatten_import(&item.tree, &[], &mut imports);
                for (path, binding) in imports {
                    match path.as_slice() {
                        [root, item] if root == "dactyl_db" && item == "Operation" => {
                            names.operations.insert(binding);
                        }
                        [root, item] if root == "dactyl_db" && item == "Connection" => {
                            names.connections.insert(binding);
                        }
                        [root, item] if root == "async_trait" && item == "async_trait" => {
                            names.async_traits.insert(binding);
                        }
                        _ if wrappers.contains(&path) => {
                            names.wrappers.insert(binding);
                        }
                        _ => {}
                    }
                }
            }
        }
        names.visit_file(&file);
        if !names.unambiguous() {
            return Self::default();
        }
        let summaries = consuming_helpers(&file, &names);
        let mut visitor = SqlVisitor {
            source,
            names: &names,
            context: Self::default(),
            impl_self: None,
            summaries,
        };
        visitor.visit_file(&file);
        visitor.context
    }

    pub(super) fn is_safe(&self, source: Range<usize>) -> bool {
        self.parameters.iter().any(|parameter| {
            parameter.token.start == source.start
                && (parameter.token.end == source.end || parameter.punctuation_end == source.end)
        })
    }
}

#[derive(Default)]
struct Names {
    operations: BTreeSet<String>,
    connections: BTreeSet<String>,
    wrappers: BTreeSet<String>,
    check_attributes: bool,
    async_traits: BTreeSet<String>,
    bindings: BTreeMap<String, usize>,
    uncertain: bool,
}

impl Names {
    fn unambiguous(&self) -> bool {
        !self.uncertain
            && !self.bindings.contains_key("dactyl_db")
            && self
                .operations
                .iter()
                .chain(&self.connections)
                .chain(&self.wrappers)
                .chain(&self.async_traits)
                .all(|name| self.bindings.get(name) == Some(&1))
            && self.bindings.get("async_trait").copied().unwrap_or(0)
                <= usize::from(self.async_traits.contains("async_trait"))
    }
    fn bind(&mut self, name: &syn::Ident) {
        let spelling = name.to_string();
        // Raw identifiers bind the same name, but are not positive evidence.
        let spelling = spelling.strip_prefix("r#").unwrap_or(&spelling);
        *self.bindings.entry(spelling.to_owned()).or_default() += 1;
    }
}

impl<'ast> Visit<'ast> for Names {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Const(item) => self.bind(&item.ident),
            Item::Enum(item) => self.bind(&item.ident),
            Item::ExternCrate(item) => {
                self.bind(
                    item.rename
                        .as_ref()
                        .map(|(_, name)| name)
                        .unwrap_or(&item.ident),
                );
                // macro_use can inject definitions without visible bindings.
                self.uncertain = true;
            }
            Item::Fn(item) => self.bind(&item.sig.ident),
            Item::Macro(_) => self.uncertain = true,
            Item::Mod(item) => self.bind(&item.ident),
            Item::Static(item) => self.bind(&item.ident),
            Item::Struct(item) => self.bind(&item.ident),
            Item::Trait(item) => self.bind(&item.ident),
            Item::TraitAlias(item) => self.bind(&item.ident),
            Item::Type(item) => self.bind(&item.ident),
            Item::Union(item) => self.bind(&item.ident),
            Item::Verbatim(_) => self.uncertain = true,
            _ => {}
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut imports = Vec::new();
        flatten_import(&item.tree, &[], &mut imports);
        for (_, binding) in imports {
            if binding == "*" {
                self.uncertain = true;
            } else {
                let binding = binding.strip_prefix("r#").unwrap_or(&binding).to_owned();
                *self.bindings.entry(binding).or_default() += 1;
            }
        }
        syn::visit::visit_item_use(self, item);
    }

    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        self.bind(&pattern.ident);
        syn::visit::visit_pat_ident(self, pattern);
    }

    fn visit_type_param(&mut self, parameter: &'ast syn::TypeParam) {
        self.bind(&parameter.ident);
        syn::visit::visit_type_param(self, parameter);
    }

    fn visit_const_param(&mut self, parameter: &'ast syn::ConstParam) {
        self.bind(&parameter.ident);
        syn::visit::visit_const_param(self, parameter);
    }

    fn visit_stmt_macro(&mut self, _: &'ast syn::StmtMacro) {
        // A statement macro can introduce a shadowing local type or import.
        self.uncertain = true;
    }

    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        let inert = [
            "cfg",
            "test",
            "allow",
            "warn",
            "deny",
            "forbid",
            "doc",
            "inline",
            "cold",
            "must_use",
            "repr",
            "non_exhaustive",
            "path",
        ];
        let known_async_trait = matches!(attribute.meta, syn::Meta::Path(_))
            && attribute
                .path()
                .get_ident()
                .is_some_and(|name| self.async_traits.contains(&name.to_string()));
        if self.check_attributes
            && !inert.iter().any(|name| attribute.path().is_ident(name))
            && !known_async_trait
        {
            // In particular derive/cfg_attr can expand arbitrary macros.
            self.uncertain = true;
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_foreign_item(&mut self, item: &'ast syn::ForeignItem) {
        match item {
            syn::ForeignItem::Fn(item) => self.bind(&item.sig.ident),
            syn::ForeignItem::Static(item) => self.bind(&item.ident),
            syn::ForeignItem::Type(item) => self.bind(&item.ident),
            _ => self.uncertain = true,
        }
        syn::visit::visit_foreign_item(self, item);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        if matches!(expression, Expr::Verbatim(_)) {
            self.uncertain = true;
        }
        syn::visit::visit_expr(self, expression);
    }
}

fn flatten_import(
    tree: &syn::UseTree,
    prefix: &[String],
    imports: &mut Vec<(Vec<String>, String)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            let mut prefix = prefix.to_vec();
            prefix.push(path.ident.to_string());
            flatten_import(&path.tree, &prefix, imports);
        }
        syn::UseTree::Name(name) => {
            let mut path = prefix.to_vec();
            let binding = if name.ident == "self" {
                prefix.last().cloned().unwrap_or_else(|| "self".to_owned())
            } else {
                path.push(name.ident.to_string());
                name.ident.to_string()
            };
            imports.push((path, binding));
        }
        syn::UseTree::Rename(rename) => {
            let mut path = prefix.to_vec();
            if rename.ident != "self" {
                path.push(rename.ident.to_string());
            }
            imports.push((path, rename.rename.to_string()));
        }
        syn::UseTree::Glob(_) => imports.push((prefix.to_vec(), "*".to_owned())),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                flatten_import(item, prefix, imports);
            }
        }
    }
}

struct SqlVisitor<'a> {
    source: &'a str,
    names: &'a Names,
    context: SqlContext,
    impl_self: Option<String>,
    summaries: BTreeMap<(String, String), Vec<usize>>,
}

impl SqlVisitor<'_> {
    fn record_literal(&mut self, expression: &Expr) {
        let Expr::Lit(literal) = expression else {
            return;
        };
        if !literal.attrs.is_empty() {
            return;
        }
        let Lit::Str(literal) = &literal.lit else {
            return;
        };
        let range = literal.token().span().byte_range();
        if let Some(original) = self.source.get(range.clone())
            && original == literal.token().to_string()
            && let Some((body, offset)) = literal_body(original)
            && let Some(parameters) = sql_parameters(body)
        {
            for parameter in parameters {
                let start = range.start + offset + parameter.start;
                let end = range.start + offset + parameter.end;
                let punctuation_end = if body.as_bytes().get(parameter.end).is_some_and(|byte| {
                    matches!(
                        byte,
                        b',' | b')'
                            | b';'
                            | b'+'
                            | b'-'
                            | b'*'
                            | b'/'
                            | b'='
                            | b'<'
                            | b'>'
                            | b':'
                            | b'%'
                    )
                }) {
                    end + 1
                } else {
                    end
                };
                self.context.parameters.push(ParameterSpan {
                    token: start..end,
                    punctuation_end,
                });
            }
        }
    }

    fn record_operation(&mut self, expression: &Expr) {
        if let Expr::Call(call) = expression
            && call.attrs.is_empty()
            && call.args.len() == 2
            && operation_constructor(&call.func, self.names)
        {
            self.record_literal(&call.args[0]);
        }
    }
}

impl<'ast> Visit<'ast> for SqlVisitor<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let previous = self.impl_self.take();
        syn::visit::visit_item_fn(self, item);
        self.impl_self = previous;
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        let previous = self.impl_self.take();
        syn::visit::visit_item_trait(self, item);
        self.impl_self = previous;
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let previous = self.impl_self.take();
        self.impl_self = simple_type(&item.self_ty);
        syn::visit::visit_item_impl(self, item);
        self.impl_self = previous;
    }

    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if let Some(array) = terminal_operations(expression, self.names) {
            for operation in &array.elems {
                self.record_operation(operation);
            }
        }
        if expression.attrs.is_empty()
            && expression.args.len() == 3
            && is_external_method(
                &expression.func,
                &self.names.connections,
                "Connection",
                &["read", "write", "write_result"],
            )
        {
            self.record_literal(&expression.args[1]);
        }
        if matches!(expression.func.as_ref(), Expr::Path(path) if path.path.leading_colon.is_none())
            && let Some(path) = expression_path(&expression.func)
            && let [owner, method] = path.as_slice()
        {
            let owner = if owner == "Self" {
                self.impl_self.as_ref()
            } else {
                Some(owner)
            };
            if let Some(owner) = owner
                && let Some(indices) = self
                    .summaries
                    .get(&(owner.clone(), method.clone()))
                    .cloned()
            {
                for index in indices {
                    if let Some(operation) = expression.args.iter().nth(index) {
                        self.record_operation(operation);
                    }
                }
            }
        }
        syn::visit::visit_expr_call(self, expression);
    }
}

fn expression_path(expression: &Expr) -> Option<Vec<String>> {
    let Expr::Path(function) = expression else {
        return None;
    };
    if function.qself.is_some()
        || !function.attrs.is_empty()
        || function
            .path
            .segments
            .iter()
            .any(|segment| !segment.arguments.is_empty())
    {
        return None;
    }
    Some(
        function
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect(),
    )
}

fn is_external_method(
    expression: &Expr,
    aliases: &BTreeSet<String>,
    class: &str,
    methods: &[&str],
) -> bool {
    let Some(path) = expression_path(expression) else {
        return false;
    };
    match path.as_slice() {
        [alias, method] => {
            aliases.contains(alias)
                && methods.contains(&method.as_str())
                && matches!(expression, Expr::Path(path) if path.path.leading_colon.is_none())
        }
        [namespace, found_class, method] => {
            namespace == "dactyl_db" && found_class == class && methods.contains(&method.as_str())
        }
        _ => false,
    }
}

fn operation_constructor(expression: &Expr, names: &Names) -> bool {
    is_external_method(
        expression,
        &names.operations,
        "Operation",
        &["read", "write"],
    )
}

fn terminal_operations<'a>(call: &'a syn::ExprCall, names: &Names) -> Option<&'a syn::ExprArray> {
    if !call.attrs.is_empty() || call.args.len() != 2 {
        return None;
    }
    let trusted = is_external_method(&call.func, &names.connections, "Connection", &["atomic"])
        || matches!(call.func.as_ref(), Expr::Path(path) if path.path.leading_colon.is_none()) && expression_path(&call.func).is_some_and(|path| matches!(path.as_slice(), [alias, method] if names.wrappers.contains(alias) && method == "atomic"));
    if !trusted {
        return None;
    }
    let Expr::Reference(reference) = &call.args[1] else {
        return None;
    };
    if reference.mutability.is_some() || !reference.attrs.is_empty() {
        return None;
    }
    let Expr::Array(array) = reference.expr.as_ref() else {
        return None;
    };
    array.attrs.is_empty().then_some(array)
}

fn simple_type(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    path.path.get_ident().map(ToString::to_string)
}

fn operation_type(ty: &syn::Type, names: &Names) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    if path.qself.is_some() || path.path.segments.iter().any(|s| !s.arguments.is_empty()) {
        return false;
    }
    let parts: Vec<_> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    matches!(parts.as_slice(), [alias] if names.operations.contains(alias))
        || matches!(parts.as_slice(), [root, item] if root == "dactyl_db" && item == "Operation")
}

fn consuming_helpers(file: &syn::File, names: &Names) -> BTreeMap<(String, String), Vec<usize>> {
    let mut summaries = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for item in &file.items {
        let Item::Impl(item) = item else {
            continue;
        };
        if item.trait_.is_some()
            || !item.generics.params.is_empty()
            || !inert_attributes(&item.attrs)
        {
            continue;
        }
        let Some(owner) = simple_type(&item.self_ty) else {
            continue;
        };
        if names.bindings.get(&owner) != Some(&1) {
            continue;
        }
        for member in &item.items {
            let syn::ImplItem::Fn(function) = member else {
                continue;
            };
            let key = (owner.clone(), function.sig.ident.to_string());
            if summaries.contains_key(&key) {
                duplicates.insert(key.clone());
            }
            let mut usage = HelperUsage {
                names,
                paths: BTreeMap::new(),
                consumed: BTreeMap::new(),
                uncertain: false,
            };
            usage.visit_block(&function.block);
            let mut indices = Vec::new();
            if !usage.uncertain
                && inert_attributes(&function.attrs)
                && function.sig.generics.params.is_empty()
                && function.sig.unsafety.is_none()
                && function.sig.asyncness.is_none()
                && function.sig.abi.is_none()
            {
                for (index, parameter) in function.sig.inputs.iter().enumerate() {
                    let syn::FnArg::Typed(parameter) = parameter else {
                        continue;
                    };
                    let syn::Pat::Ident(binding) = parameter.pat.as_ref() else {
                        continue;
                    };
                    let name = binding.ident.to_string();
                    if parameter.attrs.is_empty()
                        && binding.attrs.is_empty()
                        && binding.mutability.is_none()
                        && binding.by_ref.is_none()
                        && binding.subpat.is_none()
                        && operation_type(&parameter.ty, names)
                        && usage.paths.get(&name) == Some(&1)
                        && usage.consumed.get(&name) == Some(&1)
                    {
                        indices.push(index);
                    }
                }
            }
            summaries.insert(key, indices);
        }
    }
    for key in duplicates {
        summaries.remove(&key);
    }
    summaries
}

struct HelperUsage<'a> {
    names: &'a Names,
    paths: BTreeMap<String, usize>,
    consumed: BTreeMap<String, usize>,
    uncertain: bool,
}

impl<'ast> Visit<'ast> for HelperUsage<'_> {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if let Some(name) = path.path.get_ident() {
            *self.paths.entry(name.to_string()).or_default() += 1;
        }
        syn::visit::visit_expr_path(self, path);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Some(array) = terminal_operations(call, self.names) {
            for element in &array.elems {
                if let Expr::Path(path) = element
                    && let Some(name) = path.path.get_ident()
                {
                    *self.consumed.entry(name.to_string()).or_default() += 1;
                }
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.uncertain = true;
    }
    fn visit_pat_ident(&mut self, _: &'ast syn::PatIdent) {
        self.uncertain = true;
    }
    fn visit_expr_unsafe(&mut self, _: &'ast syn::ExprUnsafe) {
        self.uncertain = true;
    }
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {
        self.uncertain = true;
    }
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {
        self.uncertain = true;
    }
    fn visit_item(&mut self, _: &'ast Item) {
        self.uncertain = true;
    }
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
        self.uncertain = true;
    }
}

fn literal_body(original: &str) -> Option<(&str, usize)> {
    if let Some(body) = original.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        // No decoding without a verified decoded-to-source offset map.
        return (!body.contains('\\')).then_some((body, 1));
    }
    let rest = original.strip_prefix('r')?;
    let hashes = rest.bytes().take_while(|byte| *byte == b'#').count();
    let body = rest.get(hashes..)?.strip_prefix('"')?;
    let suffix = format!("\"{}", "#".repeat(hashes));
    Some((body.strip_suffix(&suffix)?, hashes + 2))
}

/// Locate bind tokens in a conservative lexical SQL subset. This does not
/// validate SQL or establish the sink: that requires trusted_constructor.
/// Strings, identifiers and comments never establish a positional parameter.
fn sql_parameters(body: &str) -> Option<Vec<Range<usize>>> {
    let bytes = body.as_bytes();
    let first = body
        .trim_start()
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()?;
    if !["SELECT", "INSERT", "UPDATE", "DELETE", "WITH"]
        .iter()
        .any(|keyword| first.eq_ignore_ascii_case(keyword))
    {
        return None;
    }
    let mut parameters = Vec::new();
    let mut index = 0;
    let mut depth = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' => {
                let quote = bytes[index];
                index += 1;
                loop {
                    match bytes.get(index) {
                        Some(byte) if *byte == quote => {
                            index += 1;
                            if bytes.get(index) == Some(&quote) {
                                index += 1;
                            } else {
                                break;
                            }
                        }
                        Some(b'\\') | None => return None,
                        Some(_) => index += 1,
                    }
                }
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index += 2;
                while bytes.get(index).is_some_and(|byte| *byte != b'\n') {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                let mut comments = 1usize;
                while comments > 0 {
                    match (bytes.get(index), bytes.get(index + 1)) {
                        (Some(b'/'), Some(b'*')) => {
                            comments += 1;
                            index += 2;
                        }
                        (Some(b'*'), Some(b'/')) => {
                            comments -= 1;
                            index += 2;
                        }
                        (None, _) => return None,
                        _ => index += 1,
                    }
                }
            }
            b'$' => {
                let start = index;
                index += 1;
                if !bytes
                    .get(index)
                    .is_some_and(|byte| matches!(byte, b'1'..=b'9'))
                    || start > 0
                        && (bytes[start - 1].is_ascii_alphanumeric()
                            || matches!(bytes[start - 1], b'_' | b'$')
                            || bytes[start - 1] >= 128)
                {
                    return None;
                }
                while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                    index += 1;
                }
                if bytes.get(index).is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || *byte >= 128
                }) {
                    return None;
                }
                parameters.push(start..index);
            }
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth = depth.checked_sub(1)?;
                index += 1;
            }
            b';' => {
                // Avoid interpreting a SQL prefix followed by shell text.
                if !body.get(index + 1..)?.trim().is_empty() {
                    return None;
                }
                index += 1;
            }
            byte if byte.is_ascii_whitespace()
                || byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b','
                        | b'.'
                        | b'+'
                        | b'-'
                        | b'*'
                        | b'/'
                        | b'='
                        | b'<'
                        | b'>'
                        | b':'
                        | b'%'
                        | b'?'
                ) =>
            {
                index += 1;
            }
            // Shell operators, substitutions, dollar quoting, backticks,
            // non-ASCII token boundaries and unfamiliar syntax stay flagged.
            _ => return None,
        }
    }
    (depth == 0).then_some(parameters)
}

// Cross-file evidence is deliberately limited to ordinary Cargo library modules.
// Custom #[path] layouts, ambiguous modules, other editions, unreadable files,
// symlinks escaping the repository and unsupported forwarding bodies decline.
fn read_rust(path: &Path, repository: &Path) -> Option<syn::File> {
    let path = path.canonicalize().ok()?;
    if !path.starts_with(repository) || std::fs::metadata(&path).ok()?.len() > 4 * 1024 * 1024 {
        return None;
    }
    let source = std::fs::read_to_string(path).ok()?;
    if source.starts_with('\u{feff}') || (source.starts_with("#!") && !source.starts_with("#![")) {
        return None;
    }
    let file = syn::parse_file(&source).ok()?;
    inert_attributes(&file.attrs).then_some(file)
}

fn inert_attributes(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().all(|attribute| {
        [
            "allow",
            "warn",
            "deny",
            "forbid",
            "doc",
            "inline",
            "cold",
            "must_use",
            "repr",
            "non_exhaustive",
        ]
        .iter()
        .any(|name| attribute.path().is_ident(name))
    })
}

fn resolve_module(root: &Path, modules: &[String], repository: &Path) -> Option<PathBuf> {
    if modules.len() > 32 {
        return None;
    }
    let mut file_path = root.canonicalize().ok()?;
    let mut directory = file_path.parent()?.to_path_buf();
    for name in modules {
        let file = read_rust(&file_path, repository)?;
        let declarations: Vec<_> = file
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Mod(module) if module.ident == name.as_str() => Some(module),
                _ => None,
            })
            .collect();
        let [module] = declarations.as_slice() else {
            return None;
        };
        if module.content.is_some() || !inert_attributes(&module.attrs) {
            return None;
        }
        let candidates: Vec<_> = [
            directory.join(format!("{name}.rs")),
            directory.join(name).join("mod.rs"),
        ]
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .collect();
        let [candidate] = candidates.as_slice() else {
            return None;
        };
        file_path = candidate.canonicalize().ok()?;
        if !file_path.starts_with(repository) {
            return None;
        }
        directory = directory.join(name);
    }
    Some(file_path)
}

// This is source-selection evidence, not cryptographic package authentication.
// Known crates.io packages and the official pinned Dactyl git repository remain
// trust anchors. Visible Cargo substitutions and unresolved inheritance decline.
fn read_manifest(path: &Path, repository: &Path) -> Option<toml::Value> {
    let path = path.canonicalize().ok()?;
    if !path.starts_with(repository) || std::fs::metadata(&path).ok()?.len() > 1024 * 1024 {
        return None;
    }
    toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn trusted_dependency(value: &toml::Value, name: &str) -> bool {
    if let Some(version) = value.as_str() {
        return !version.is_empty();
    }
    let Some(table) = value.as_table() else {
        return false;
    };
    if [
        "path",
        "package",
        "registry",
        "registry-index",
        "workspace",
        "branch",
        "tag",
    ]
    .iter()
    .any(|key| table.contains_key(*key))
    {
        return false;
    }
    if let Some(git) = table.get("git") {
        name == "dactyl-db"
            && git.as_str() == Some("https://github.com/DecapodLabs/dactyl.git")
            && table
                .get("rev")
                .and_then(toml::Value::as_str)
                .is_some_and(|revision| {
                    revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
    } else {
        !table.contains_key("rev")
            && table
                .get("version")
                .and_then(toml::Value::as_str)
                .is_some_and(|version| !version.is_empty())
    }
}

fn dependency_entry<'a>(table: &'a toml::Value, name: &str) -> Option<&'a toml::Value> {
    let entries = table.as_table()?;
    let underscored = name.replace('-', "_");
    match (entries.get(name), entries.get(&underscored)) {
        (Some(value), None) | (None, Some(value)) => Some(value),
        _ => None,
    }
}

fn trusted_dependencies(
    manifest: &toml::Value,
    directory: &Path,
    repository: &Path,
    needs_async_trait: bool,
) -> bool {
    if manifest
        .get("package")
        .and_then(|package| package.get("workspace"))
        .is_some()
    {
        return false;
    }
    let mut parents = Vec::new();
    let mut current = Some(directory);
    while let Some(path) = current.filter(|path| path.starts_with(repository)) {
        let candidate = path.join("Cargo.toml");
        if candidate.is_file() {
            let Some(parent) = read_manifest(&candidate, repository) else {
                return false;
            };
            if ["patch", "replace"].iter().any(|key| {
                parent
                    .get(*key)
                    .and_then(toml::Value::as_table)
                    .is_some_and(|table| !table.is_empty())
            }) {
                return false;
            }
            parents.push(parent);
        }
        current = path.parent();
    }
    let inherited = parents.iter().find_map(|parent| parent.get("workspace"));
    let resolve = |value: &toml::Value, name: &str| {
        if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
            if value.as_table().is_none_or(|table| {
                table.keys().any(|key| {
                    !["workspace", "features", "default-features", "optional"]
                        .contains(&key.as_str())
                })
            }) {
                return false;
            }
            inherited
                .and_then(|workspace| workspace.get("dependencies"))
                .and_then(|table| dependency_entry(table, name))
                .is_some_and(|value| trusted_dependency(value, name))
        } else {
            trusted_dependency(value, name)
        }
    };
    let Some(dependencies) = manifest.get("dependencies") else {
        return false;
    };
    if !dependency_entry(dependencies, "dactyl-db").is_some_and(|value| resolve(value, "dactyl-db"))
    {
        return false;
    }
    if needs_async_trait
        && !dependency_entry(dependencies, "async-trait")
            .is_some_and(|value| resolve(value, "async-trait"))
    {
        return false;
    }
    let mut tables = vec![manifest];
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        tables.extend(targets.values());
    }
    for owner in tables {
        for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(table) = owner.get(key) {
                for name in ["dactyl-db", "async-trait"] {
                    if table.get(name).is_some() && table.get(name.replace('-', "_")).is_some() {
                        return false;
                    }
                    if let Some(value) = dependency_entry(table, name)
                        && !resolve(value, name)
                    {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn owning_crate_root(
    repository: &Path,
    source_path: &Path,
    needs_async_trait: bool,
) -> Option<PathBuf> {
    let repository = repository.canonicalize().ok()?;
    let source_path = repository.join(source_path).canonicalize().ok()?;
    if !source_path.starts_with(&repository) {
        return None;
    }
    let mut directory = source_path.parent()?;
    loop {
        let manifest_path = directory.join("Cargo.toml");
        if manifest_path.is_file() {
            let manifest = read_manifest(&manifest_path, &repository)?;
            if !trusted_dependencies(&manifest, directory, &repository, needs_async_trait) {
                return None;
            }
            let edition = manifest.get("package")?.get("edition")?.as_str()?;
            if !matches!(edition, "2018" | "2021" | "2024") {
                return None;
            }
            let library = manifest
                .get("lib")
                .and_then(|lib| lib.get("path"))
                .and_then(toml::Value::as_str)
                .unwrap_or("src/lib.rs");
            let root = directory.join(library).canonicalize().ok()?;
            if !root.starts_with(&repository) {
                return None;
            }
            if root == source_path {
                return Some(root);
            }
            let relative = source_path.strip_prefix(root.parent()?).ok()?;
            let mut modules: Vec<_> = relative
                .components()
                .map(|part| part.as_os_str().to_str().map(str::to_owned))
                .collect::<Option<_>>()?;
            let file = modules.pop()?;
            if file != "mod.rs" {
                modules.push(file.strip_suffix(".rs")?.to_owned());
            }
            return (resolve_module(&root, &modules, &repository)? == source_path).then_some(root);
        }
        directory = directory.parent()?;
        if !directory.starts_with(&repository) {
            return None;
        }
    }
}

fn verified_wrappers(repository: &Path, root: &Path, source: &str) -> BTreeSet<Vec<String>> {
    let Some(repository) = repository.canonicalize().ok() else {
        return BTreeSet::new();
    };
    let Some(file) = syn::parse_file(source).ok() else {
        return BTreeSet::new();
    };
    let mut owners = AtomicOwners(BTreeSet::new());
    owners.visit_file(&file);
    let mut verified = BTreeSet::new();
    for item in &file.items {
        let Item::Use(item) = item else {
            continue;
        };
        if !item.attrs.is_empty() {
            continue;
        }
        let mut imports = Vec::new();
        flatten_import(&item.tree, &[], &mut imports);
        for (path, binding) in imports {
            if !owners.0.contains(&binding) {
                continue;
            }
            if path.first().is_none_or(|root| root != "crate") || path.len() < 2 {
                continue;
            }
            let Some(module) = resolve_module(root, &path[1..path.len() - 1], &repository) else {
                continue;
            };
            let Some(module) = read_rust(&module, &repository) else {
                continue;
            };
            if verified_atomic_wrapper(&module, &path[path.len() - 1]) {
                verified.insert(path);
            }
        }
    }
    verified
}

struct AtomicOwners(BTreeSet<String>);

impl<'ast> Visit<'ast> for AtomicOwners {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Some(path) = expression_path(&call.func)
            && let [owner, method] = path.as_slice()
            && method == "atomic"
        {
            self.0.insert(owner.clone());
        }
        syn::visit::visit_expr_call(self, call);
    }
}

fn verified_atomic_wrapper(file: &syn::File, name: &str) -> bool {
    let mut connections = BTreeSet::new();
    let mut operations = BTreeSet::new();
    let mut bindings = BTreeMap::<String, usize>::new();
    for item in &file.items {
        if let Item::Use(item) = item {
            let mut imports = Vec::new();
            flatten_import(&item.tree, &[], &mut imports);
            for (path, binding) in imports {
                if binding == "*" {
                    return false;
                }
                *bindings.entry(binding.clone()).or_default() += 1;
                if item.leading_colon.is_some() && item.attrs.is_empty() {
                    match path.as_slice() {
                        [root, item] if root == "dactyl_db" && item == "Connection" => {
                            connections.insert(binding);
                        }
                        [root, item] if root == "dactyl_db" && item == "Operation" => {
                            operations.insert(binding);
                        }
                        _ => {}
                    }
                }
            }
        } else {
            let ident = match item {
                Item::Const(item) => Some(&item.ident),
                Item::Enum(item) => Some(&item.ident),
                Item::Fn(item) => Some(&item.sig.ident),
                Item::Mod(item) => Some(&item.ident),
                Item::Static(item) => Some(&item.ident),
                Item::Struct(item) => Some(&item.ident),
                Item::Trait(item) => Some(&item.ident),
                Item::TraitAlias(item) => Some(&item.ident),
                Item::Type(item) => Some(&item.ident),
                Item::Union(item) => Some(&item.ident),
                Item::Macro(_) | Item::ExternCrate(_) | Item::Verbatim(_) => return false,
                _ => None,
            };
            if let Some(ident) = ident {
                let ident = ident.to_string();
                *bindings
                    .entry(ident.strip_prefix("r#").unwrap_or(&ident).to_owned())
                    .or_default() += 1;
            }
        }
    }
    if connections.is_empty()
        || operations.is_empty()
        || connections
            .iter()
            .chain(&operations)
            .any(|alias| bindings.get(alias) != Some(&1))
        || bindings.get(name) != Some(&1)
    {
        return false;
    }
    let Some(structure) = file.items.iter().find_map(|item| match item {
        Item::Struct(item) if item.ident == name => Some(item),
        _ => None,
    }) else {
        return false;
    };
    if !structure.generics.params.is_empty() || !inert_attributes(&structure.attrs) {
        return false;
    }
    let connection_fields: BTreeSet<_> = structure
        .fields
        .iter()
        .filter_map(|field| {
            (inert_attributes(&field.attrs)
                && simple_type(&field.ty).is_some_and(|ty| connections.contains(&ty)))
            .then(|| field.ident.as_ref().map(ToString::to_string))
            .flatten()
        })
        .collect();
    let mut methods = Vec::new();
    for item in &file.items {
        let Item::Impl(item) = item else {
            continue;
        };
        if item.trait_.is_some() || simple_type(&item.self_ty).as_deref() != Some(name) {
            continue;
        }
        if !item.generics.params.is_empty() || !inert_attributes(&item.attrs) {
            return false;
        }
        for member in &item.items {
            if let syn::ImplItem::Fn(method) = member
                && method.sig.ident == "atomic"
            {
                methods.push(method);
            }
        }
    }
    let [method] = methods.as_slice() else {
        return false;
    };
    if !inert_attributes(&method.attrs)
        || !method.sig.generics.params.is_empty()
        || method.sig.unsafety.is_some()
        || method.sig.asyncness.is_some()
        || method.sig.abi.is_some()
        || method.sig.constness.is_some()
        || method.sig.inputs.len() != 2
    {
        return false;
    }
    let syn::FnArg::Receiver(receiver) = &method.sig.inputs[0] else {
        return false;
    };
    if receiver.reference.is_none()
        || receiver.mutability.is_some()
        || receiver.colon_token.is_some()
        || !receiver.attrs.is_empty()
    {
        return false;
    }
    let syn::FnArg::Typed(parameter) = &method.sig.inputs[1] else {
        return false;
    };
    let syn::Pat::Ident(binding) = parameter.pat.as_ref() else {
        return false;
    };
    if !parameter.attrs.is_empty()
        || !binding.attrs.is_empty()
        || binding.mutability.is_some()
        || binding.by_ref.is_some()
        || binding.subpat.is_some()
    {
        return false;
    }
    let syn::Type::Reference(reference) = parameter.ty.as_ref() else {
        return false;
    };
    let syn::Type::Slice(slice) = reference.elem.as_ref() else {
        return false;
    };
    if reference.mutability.is_some()
        || !simple_type(&slice.elem).is_some_and(|ty| operations.contains(&ty))
    {
        return false;
    }
    let [syn::Stmt::Expr(Expr::Call(ok), None)] = method.block.stmts.as_slice() else {
        return false;
    };
    if expression_path(&ok.func) != Some(vec!["Ok".to_owned()])
        || ok.args.len() != 1
        || !ok.attrs.is_empty()
    {
        return false;
    }
    let Expr::Try(try_expression) = &ok.args[0] else {
        return false;
    };
    if !try_expression.attrs.is_empty() {
        return false;
    }
    let Expr::MethodCall(call) = try_expression.expr.as_ref() else {
        return false;
    };
    if call.method != "atomic"
        || call.turbofish.is_some()
        || !call.attrs.is_empty()
        || call.args.len() != 1
    {
        return false;
    }
    let Expr::Field(field) = call.receiver.as_ref() else {
        return false;
    };
    let syn::Member::Named(field_name) = &field.member else {
        return false;
    };
    field.attrs.is_empty()
        && connection_fields.contains(&field_name.to_string())
        && expression_path(&field.base) == Some(vec!["self".to_owned()])
        && expression_path(&call.args[0]) == Some(vec![binding.ident.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe(source: &str, matched: &str) -> bool {
        let start = source.find(matched).expect("match in fixture");
        SqlContext::parse(source).is_safe(start..start + matched.len())
    }

    fn direct(sql_literal: &str) -> String {
        format!(
            "use dactyl_db::{{Connection, Operation}}; fn f() {{ Connection::atomic(&db, &[Operation::read({sql_literal}, vec![])]); }}"
        )
    }

    #[test]
    fn terminal_connection_calls_prove_exact_positional_tokens() {
        let source = direct(r#""INSERT INTO t VALUES ($1, $20)""#);
        for token in ["$1,", "$20)", "$20"] {
            assert!(safe(&source, token), "{token}");
        }
        for token in ["$2", "VALUES ($1,", "$1, $20)"] {
            assert!(!safe(&source, token));
        }
        let source =
            r#"use dactyl_db::Connection as Db; fn f() { Db::read(&db, "SELECT $1, $2", &[]); }"#;
        assert!(safe(source, "$1,"));
        assert!(safe(
            r#"fn f() { ::dactyl_db::Connection::write(&db, "SELECT $1, $2", &[]); }"#,
            "$1,"
        ));
    }

    #[test]
    fn constructors_and_indirect_shell_flows_are_not_sinks() {
        for body in [
            r#"let query = "SELECT $1, $2"; run_external(query);"#,
            r#"let operation = Operation::read("SELECT $1, $2", vec![]); run_external(operation.sql());"#,
            r#"run_external(Operation::read("SELECT $1, $2", vec![]).sql());"#,
            r#"Operation::read("SELECT $1, $2", vec![]);"#,
            r#"fake.atomic(&[Operation::read("SELECT $1, $2", vec![])]);"#,
            r#"let ops = [Operation::read("SELECT $1, $2", vec![])]; Connection::atomic(&db, &ops); run_external(ops[0].sql());"#,
            r#"custom!(Connection::atomic(&db, &[Operation::read("SELECT $1, $2", vec![])]));"#,
            r#"Connection::atomic(&db, &vec![Operation::read("SELECT $1, $2", vec![])]);"#,
            r#"let mut sql = "SELECT $1, $2"; sql = shell(); Connection::read(&db, sql, &[]);"#,
        ] {
            let source = format!("use dactyl_db::{{Connection, Operation}}; fn f() {{ {body} }}");
            assert!(!safe(&source, "$1,"), "{body}");
        }
    }

    #[test]
    fn namespace_shadowing_and_unknown_expansion_fail_closed() {
        for prefix in [
            "mod dactyl_db {}",
            "use impostor::Connection;",
            "use impostor::*;",
            "struct Operation;",
            "type Connection = impostor::Connection;",
            "extern crate impostor as dactyl_db;",
            "use impostor as dactyl_db;",
            "mod inner { use impostor::Operation; }",
            "mod r#dactyl_db {}",
            "macro_rules! shadow { () => {} }",
            "include!(\"bindings.rs\");",
            "#[custom] struct Other;",
            "#[derive(Custom)] struct Other;",
            "#[cfg_attr(feature=\"x\", custom)] struct Other;",
            "unsafe extern \"C\" { type Connection; }",
        ] {
            let source = format!("{prefix} {}", direct(r#""SELECT $1, $2""#));
            assert!(!safe(&source, "$1,"), "{prefix}");
        }
        for source in [
            r#"use dactyl_db::Connection; fn f<Connection>() { Connection::read(&db, "SELECT $1, $2", &[]); }"#,
            r#"use dactyl_db::Connection; fn f() { shadow!(); Connection::read(&db, "SELECT $1, $2", &[]); }"#,
            r#"use dactyl_db::Connection; fn f() { let Connection = fake; Connection::read(&db, "SELECT $1, $2", &[]); }"#,
        ] {
            assert!(!safe(source, "$1,"));
        }
    }

    #[test]
    fn async_trait_requires_unambiguous_import() {
        let item = r#"#[async_trait] impl Store for Db { async fn save(&self) { Connection::read(&db, "SELECT $1, $2", &[]); } }"#;
        assert!(safe(
            &format!("use dactyl_db::Connection; use async_trait::async_trait; {item}"),
            "$1,"
        ));
        for prefix in [
            "",
            "use impostor::async_trait;",
            "use async_trait::async_trait; mod async_trait {}",
        ] {
            assert!(!safe(
                &format!("use dactyl_db::Connection; {prefix} {item}"),
                "$1,"
            ));
        }
    }

    #[test]
    fn safe_consuming_helpers_do_not_allow_observation_or_escape() {
        let helper = "Connection::atomic(db, &[operation])";
        let source = format!(
            r#"use dactyl_db::{{Connection, Operation}}; struct Store; impl Store {{ fn consume(db: &Connection, operation: Operation) {{ {helper}; }} fn f() {{ Self::consume(&db, Operation::read("SELECT $1, $2", vec![])); }} }}"#
        );
        assert!(safe(&source, "$1,"));
        for body in [
            "run_external(operation.sql()); Connection::atomic(db, &[operation]);",
            "run_external(operation.sql());",
            "unknown!(); Connection::atomic(db, &[operation]);",
            "let alias = operation; Connection::atomic(db, &[alias]);",
            "let operation = substitute(); Connection::atomic(db, &[operation]);",
            "let delayed = || Connection::atomic(db, &[operation]);",
            "unsafe { Connection::atomic(db, &[operation]); }",
        ] {
            let source = format!(
                r#"use dactyl_db::{{Connection, Operation}}; struct Store; impl Store {{ fn consume(db: &Connection, operation: Operation) {{ {body} }} fn f() {{ Self::consume(&db, Operation::read("SELECT $1, $2", vec![])); }} }}"#
            );
            assert!(!safe(&source, "$1,"), "{body}");
        }
    }

    #[test]
    fn consumer_owner_shadowing_does_not_borrow_an_unrelated_summary() {
        let helper = "use dactyl_db::{Connection, Operation}; struct Trusted; impl Trusted { fn consume(db: &Connection, operation: Operation) { Connection::atomic(db, &[operation]); } }";
        for attack in [
            r#"fn attack<Trusted: Evil>() { Trusted::consume(&db, Operation::read("SELECT $1, $2", vec![])); }"#,
            r#"fn attack() { struct Trusted; Trusted::consume(&db, Operation::read("SELECT $1, $2", vec![])); }"#,
            r#"fn attack() { use evil::Trusted; Trusted::consume(&db, Operation::read("SELECT $1, $2", vec![])); }"#,
            r#"mod evil { struct Trusted; fn attack() { Trusted::consume(&db, Operation::read("SELECT $1, $2", vec![])); } }"#,
        ] {
            assert!(!safe(&format!("{helper} {attack}"), "$1,"), "{attack}");
        }
    }

    #[test]
    fn nested_trait_self_is_not_the_enclosing_consumer_type() {
        let source = r#"use dactyl_db::{Connection, Operation}; struct Store;
            impl Store {
                fn consume(db: &Connection, operation: Operation) { Connection::atomic(db, &[operation]); }
                fn outer() {
                    trait Evil {
                        fn consume(db: &Connection, operation: Operation);
                        fn attack() { Self::consume(&db, Operation::read("SELECT $1, $2", vec![])); }
                    }
                }
            }"#;
        assert!(!safe(source, "$1,"));
    }

    #[test]
    fn conditional_helper_cannot_lend_its_body_to_a_trait_fallback() {
        let source = r#"use dactyl_db::{Connection, Operation}; struct Store;
            impl Store { #[cfg(any())] fn consume(db: &Connection, operation: Operation) { Connection::atomic(db, &[operation]); } }
            trait Evil { fn consume(db: &Connection, operation: Operation); }
            impl Evil for Store { fn consume(db: &Connection, operation: Operation) { run_external(operation.sql()); } }
            fn f() { Store::consume(&db, Operation::read("SELECT $1, $2", vec![])); }"#;
        assert!(!safe(source, "$1,"));
    }

    #[test]
    fn exact_offsets_cover_raw_multiline_unicode_and_crlf() {
        let source = format!(
            "// π\r\n{}",
            direct("r###\"WITH t AS (\r\nSELECT $1, $2) SELECT * FROM t\"###")
        );
        assert!(safe(&source, "$1,"));
        assert!(safe(&source, "$2)"));
        for prefix in ["\u{feff}", "#!/usr/bin/rust-script\n"] {
            assert!(!safe(&format!("{prefix}{source}"), "$1,"));
        }
        assert!(!safe(&direct(r#""SELECT $1,\n $2""#), "$1,"));
        assert!(!safe("fn broken( SELECT $1,", "$1,"));
    }

    #[test]
    fn quoted_comments_and_shell_occurrences_stay_flagged() {
        let source = direct(
            r##"r#"SELECT $1, '$2,', "$3,", $4 /* $5, */ -- $6,
            FROM t"#"##,
        );
        assert!(safe(&source, "$1,"));
        for token in ["$2,", "$3,", "$5,", "$6,"] {
            assert!(!safe(&source, token));
        }
        for sql in [
            "echo $1, $2",
            "SELECT $1, $(whoami)",
            "SELECT $1, ${HOME}",
            "SELECT $1; echo $2;",
            "SELECT $1, `echo $2`",
            "SELECT $1, $2abc",
            "SELECT $1, $0",
            "SELECT $1, $01",
            "SELECT $1, x$2",
            "SELECT $1, $2 && echo hi",
            "SELECT $1, ($2",
            "SELECT $1, 'open",
        ] {
            assert!(!safe(&direct(&format!("\"{sql}\"")), "$1"), "{sql}");
        }
    }

    #[test]
    fn independent_same_line_literals_are_independent() {
        let source = r#"use dactyl_db::Connection; fn f() { Connection::read(&db, "SELECT $1, $2", &[]); run_external("echo $3;"); }"#;
        assert!(safe(source, "$1,"));
        assert!(!safe(source, "$3;"));
    }

    const WRAPPER: &str = r#"use ::dactyl_db::{Connection, Operation};
        pub struct Bridge { connection: Connection }
        impl Bridge { pub fn atomic(&self, operations: &[Operation]) -> Result<AtomicResult, Error> {
            Ok(self.connection.atomic(operations)?)
        } }"#;
    const CALLER: &str = r#"use crate::storage::Bridge; use ::dactyl_db::Operation;
        fn f() { Bridge::atomic(&bridge, &[Operation::read("SELECT $1, $2", vec![])]); }"#;

    fn repository(wrapper: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndactyl-db='0.10'\n",
        )
        .unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "mod storage; mod caller;").unwrap();
        std::fs::write(root.path().join("src/storage.rs"), wrapper).unwrap();
        std::fs::write(root.path().join("src/caller.rs"), CALLER).unwrap();
        root
    }

    fn repository_safe(root: &Path) -> bool {
        let start = CALLER.find("$1,").unwrap();
        SqlContext::parse_file(root, Path::new("src/caller.rs"), CALLER).is_safe(start..start + 3)
    }

    #[test]
    fn wrapper_proof_resolves_declared_module_and_exact_terminal_forwarding() {
        let root = repository(WRAPPER);
        assert!(repository_safe(root.path()));
        for wrapper in [
            WRAPPER.replace("use ::dactyl_db", "use impostor"),
            WRAPPER.replace("use ::dactyl_db", "use dactyl_db"),
            WRAPPER.replace("connection: Connection", "connection: Fake"),
            WRAPPER.replace(
                "Ok(self.connection.atomic(operations)?)",
                "run_external(operations[0].sql()); Ok(self.connection.atomic(operations)?)",
            ),
            WRAPPER.replace(
                "Ok(self.connection.atomic(operations)?)",
                "Ok(fake(operations))",
            ),
            WRAPPER.replace("impl Bridge", "#[custom] impl Bridge"),
            WRAPPER.replace("impl Bridge", "#[cfg(any())] impl Bridge"),
            WRAPPER.replace("pub struct Bridge", "#[cfg(any())] pub struct Bridge"),
            WRAPPER.replace("pub fn atomic", "#[cfg(any())] pub fn atomic"),
            WRAPPER.replace("pub fn atomic", "pub async fn atomic"),
            WRAPPER.replace("pub fn atomic", "pub unsafe fn atomic"),
            format!("use custom::*; {WRAPPER}"),
            format!("struct Connection; {WRAPPER}"),
        ] {
            let root = repository(&wrapper);
            assert!(!repository_safe(root.path()), "{wrapper}");
        }
    }

    #[test]
    fn missing_ambiguous_custom_or_old_crate_context_fails_closed() {
        for (file, contents) in [
            ("src/lib.rs", "mod other;"),
            ("src/lib.rs", "#[custom] mod storage; mod caller;"),
            ("src/lib.rs", "#[cfg(any())] mod storage; mod caller;"),
            ("src/lib.rs", "mod storage; mod storage; mod caller;"),
            ("src/storage.rs", "fn broken("),
            (
                "Cargo.toml",
                "[package]\nname='fixture'\nversion='0.1.0'\nedition='2015'\n",
            ),
        ] {
            let root = repository(WRAPPER);
            std::fs::write(root.path().join(file), contents).unwrap();
            assert!(!repository_safe(root.path()), "{file}: {contents}");
        }
        let root = repository(WRAPPER);
        std::fs::remove_file(root.path().join("src/storage.rs")).unwrap();
        assert!(!repository_safe(root.path()));
    }

    #[test]
    fn cargo_dependency_substitution_is_not_sql_evidence() {
        for dependency in [
            "{ path = '../evil' }",
            "{ package = 'evil', version = '1' }",
            "{ registry = 'evil', version = '1' }",
            "{ git = 'https://example.invalid/evil', rev = '0123456789012345678901234567890123456789' }",
            "{ workspace = true }",
        ] {
            let root = repository(WRAPPER);
            std::fs::write(root.path().join("Cargo.toml"), format!("[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndactyl-db={dependency}\n")).unwrap();
            assert!(!repository_safe(root.path()), "{dependency}");
        }
        for addition in [
            "[patch.crates-io]\ndactyl-db={path='../evil'}\n",
            "[replace]\n'dactyl-db:0.10.0'={path='../evil'}\n",
            "[target.'cfg(unix)'.dependencies]\ndactyl-db={path='../evil'}\n",
        ] {
            let root = repository(WRAPPER);
            let manifest = std::fs::read_to_string(root.path().join("Cargo.toml")).unwrap();
            std::fs::write(
                root.path().join("Cargo.toml"),
                format!("{manifest}{addition}"),
            )
            .unwrap();
            assert!(!repository_safe(root.path()), "{addition}");
        }
    }

    #[test]
    fn workspace_dependency_inheritance_is_resolved_and_overrides_rejected() {
        let root = repository(WRAPPER);
        let manifest = "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndactyl-db={workspace=true}\n[workspace.dependencies]\ndactyl-db='0.10'\n";
        std::fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
        assert!(repository_safe(root.path()));
        std::fs::write(
            root.path().join("Cargo.toml"),
            manifest.replace("dactyl-db='0.10'", "dactyl-db={path='../evil'}"),
        )
        .unwrap();
        assert!(!repository_safe(root.path()));
    }

    #[test]
    fn actual_dactyl_todo_matches_have_consumed_sql_evidence() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = Path::new("src/decapod/core/dactyl_todo.rs");
        let source = include_str!("../dactyl_todo.rs");
        let context = SqlContext::parse_file(root, path, source);
        let pattern = fancy_regex::Regex::new(r#"\$\w+[^\s"']"#).unwrap();
        let mut count = 0;
        for matched in pattern.find_iter(source) {
            let matched = matched.unwrap();
            assert!(
                context.is_safe(matched.range()),
                "{} at {}",
                matched.as_str(),
                matched.start()
            );
            count += 1;
        }
        assert!(count >= 10);
    }
}
