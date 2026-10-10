//! Conservative source evidence for Rust formatting expressions.
//!
//! This is not macro expansion or whole-program dataflow. Unknown syntax and
//! sources retain findings. In particular, a replacement field alone is not
//! proof that the supplied credential was not hardcoded elsewhere.

use std::collections::BTreeMap;
use std::ops::Range;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, Lit, Token};

#[derive(Default)]
pub(super) struct RustContext {
    fields: Vec<FormatField>,
    uncertain_expansion: bool,
    proven_runtime: Vec<Range<usize>>,
    runtime_initializers: Vec<Range<usize>>,
}

struct FormatField {
    source: Range<usize>,
    runtime_source: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PasswordContext {
    RuntimeEnvironment,
    UnresolvedFormat,
    LiteralOrUnknown,
}

impl RustContext {
    pub(super) fn parse(source: &str) -> Self {
        // syn removes these prefixes before producing token spans. Decline
        // classification instead of guessing offsets into the original file.
        if source.starts_with('\u{feff}')
            || (source.starts_with("#!") && !source.starts_with("#!["))
        {
            return Self::default();
        }
        let Ok(file) = syn::parse_file(source) else {
            return Self::default();
        };
        let mut visitor = ContextVisitor {
            source,
            context: Self::default(),
            captures: BTreeMap::new(),
            captures_allowed: false,
        };
        visitor.visit_file(&file);
        visitor.context
    }

    pub(super) fn parse_file(
        repo_root: &std::path::Path,
        path: &std::path::Path,
        source: &str,
    ) -> Self {
        if super::source_dependencies::manifest_standard_library_ambiguous(repo_root, path) {
            return Self::default();
        }
        let mut context = Self::parse(source);
        if !source.starts_with('\u{feff}')
            && (!source.starts_with("#!") || source.starts_with("#!["))
            && super::source_dependencies::modern_crate_root(repo_root, path)
        {
            context.proven_runtime = super::runtime_credentials::fields(source);
        }
        context
    }

    pub(super) fn password_value(&self, source: Range<usize>) -> PasswordContext {
        if self.proven_runtime.contains(&source) {
            return PasswordContext::RuntimeEnvironment;
        }
        if self.uncertain_expansion {
            return PasswordContext::LiteralOrUnknown;
        }
        if self
            .runtime_initializers
            .iter()
            .any(|initializer| source.start == initializer.start && source.end <= initializer.end)
        {
            return PasswordContext::RuntimeEnvironment;
        }
        match self.fields.iter().find(|field| field.source == source) {
            Some(field) if field.runtime_source => PasswordContext::RuntimeEnvironment,
            Some(_) => PasswordContext::UnresolvedFormat,
            None => PasswordContext::LiteralOrUnknown,
        }
    }
}

struct ContextVisitor<'a> {
    source: &'a str,
    context: RustContext,
    captures: BTreeMap<String, bool>,
    captures_allowed: bool,
}

impl<'ast> Visit<'ast> for ContextVisitor<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let outer = std::mem::take(&mut self.captures);
        let previous = self.captures_allowed;
        let mut hazards = CaptureHazards(false);
        hazards.visit_block(&item.block);
        self.captures_allowed = !hazards.0;
        syn::visit::visit_item_fn(self, item);
        self.captures = outer;
        self.captures_allowed = previous;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let outer = std::mem::take(&mut self.captures);
        let previous = self.captures_allowed;
        let mut hazards = CaptureHazards(false);
        hazards.visit_block(&item.block);
        self.captures_allowed = !hazards.0;
        syn::visit::visit_impl_item_fn(self, item);
        self.captures = outer;
        self.captures_allowed = previous;
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        let outer = self.captures.clone();
        syn::visit::visit_block(self, block);
        self.captures = outer;
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        // Visit the initializer before introducing the binding. A binding's
        // own name must never be used as evidence for its initializer.
        let runtime = local.attrs.is_empty()
            && local
                .init
                .as_ref()
                .is_some_and(|init| init.diverge.is_none() && self.runtime_value(&init.expr));
        if runtime && let Some(initializer) = &local.init {
            // Only a candidate beginning at the actual expression start can
            // use this proof. A credential literal inside an environment key
            // or another nested argument must retain its own finding.
            let expression = initializer.expr.span().byte_range();
            // The legacy unquoted matcher includes an adjacent semicolon in
            // its candidate. Only this parsed local's terminator can extend
            // the proof; a later assignment or nested literal cannot.
            self.context
                .runtime_initializers
                .push(expression.start..local.semi_token.span.byte_range().end);
        }
        syn::visit::visit_local(self, local);
        if let syn::Pat::Ident(binding) = &local.pat {
            let name = binding.ident.to_string();
            self.captures.insert(
                name.clone(),
                runtime
                    && binding.attrs.is_empty()
                    && binding.by_ref.is_none()
                    && binding.mutability.is_none()
                    && binding.subpat.is_none()
                    && !name.starts_with("r#"),
            );
        }
    }

    fn visit_pat_ident(&mut self, pattern: &'ast syn::PatIdent) {
        // Function/closure parameters, match arms and destructuring can all
        // shadow a proven local. Unsupported patterns erase the evidence.
        self.captures.remove(&pattern.ident.to_string());
        syn::visit::visit_pat_ident(self, pattern);
    }

    fn visit_expr_closure(&mut self, expression: &'ast syn::ExprClosure) {
        // Do not transfer local provenance across a deferred execution scope.
        let outer = std::mem::take(&mut self.captures);
        let previous = self.captures_allowed;
        self.captures_allowed = false;
        syn::visit::visit_expr_closure(self, expression);
        self.captures = outer;
        self.captures_allowed = previous;
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        // Source files can be scanned without their Cargo edition. In older
        // editions, absolute paths can resolve a local crate-root namespace.
        if reserved_namespace(&item.ident) {
            self.context.uncertain_expansion = true;
        }
        syn::visit::visit_item_mod(self, item);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if ambiguous_namespace_import(&item.tree) {
            self.context.uncertain_expansion = true;
        }
        syn::visit::visit_item_use(self, item);
    }

    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        // These built-in attributes do not rewrite the visited expression.
        // cfg_attr and derive are intentionally not included: they can select
        // arbitrary procedural attributes or generate additional code.
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
        ];
        if !inert.iter().any(|name| attribute.path().is_ident(name)) {
            self.context.uncertain_expansion = true;
        }
        syn::visit::visit_attribute(self, attribute);
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        let name = item
            .rename
            .as_ref()
            .map(|(_, name)| name)
            .unwrap_or(&item.ident);
        if reserved_namespace(name) {
            self.context.uncertain_expansion = true;
        }
        syn::visit::visit_item_extern_crate(self, item);
    }

    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let names: Vec<_> = invocation
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let format_names = [
            "format",
            "format_args",
            "print",
            "println",
            "eprint",
            "eprintln",
        ];
        let recognized = match names.as_slice() {
            [name] => format_names.contains(&name.as_str()),
            [namespace, name] => {
                namespace == "std" && format_names.contains(&name.as_str())
                    || namespace == "core" && name == "format_args"
            }
            _ => false,
        };
        if !recognized {
            return;
        }
        // An unqualified name may refer to a custom macro. It can receive a
        // diagnostic, but never establishes a runtime-source exemption.
        let standard = invocation.path.leading_colon.is_some()
            && names.len() == 2
            && invocation
                .path
                .segments
                .iter()
                .all(|segment| segment.arguments.is_empty());
        let Ok(arguments) =
            Punctuated::<Expr, Token![,]>::parse_terminated.parse2(invocation.tokens.clone())
        else {
            return;
        };
        // Attributes inside macro input are not visited by the file visitor.
        // Reject the complete invocation if any parsed argument is attributed;
        // dropping just that argument could hide an ambiguous duplicate name.
        let mut attributes = ArgumentAttributes(false);
        for argument in &arguments {
            attributes.visit_expr(argument);
        }
        if attributes.0 {
            return;
        }
        let Some(Expr::Lit(literal)) = arguments.first() else {
            return;
        };
        if !literal.attrs.is_empty() {
            return;
        }
        let Lit::Str(literal) = &literal.lit else {
            return;
        };
        // The direct proc-macro2 dependency enables its span-locations API.
        let span: proc_macro2::Span = literal.token().span();
        let span = span.byte_range();
        let Some(original) = self.source.get(span.clone()) else {
            return;
        };
        if original != literal.token().to_string() {
            return;
        }
        let Some((body, body_offset)) = literal_body(original) else {
            return;
        };
        let Some(fields) = replacement_fields(body) else {
            return;
        };
        // Validate the complete supported argument grammar before trusting any
        // argument. Discarding unsupported or duplicate bindings would make
        // the remaining safe-looking subset misleading evidence.
        let mut bindings = BTreeMap::new();
        for argument in arguments.iter().skip(1) {
            let Expr::Assign(assignment) = argument else {
                return;
            };
            let Expr::Path(path) = assignment.left.as_ref() else {
                return;
            };
            if path.qself.is_some() {
                return;
            }
            let Some(name) = path.path.get_ident() else {
                return;
            };
            let name = name.to_string();
            if name.starts_with("r#")
                || !fields.iter().any(|(_, field)| *field == name)
                || bindings.insert(name, assignment.right.as_ref()).is_some()
            {
                return;
            }
        }
        for (field, name) in fields {
            let runtime_source = standard
                && match bindings.get(name) {
                    Some(value) => self.runtime_value(value),
                    None => {
                        self.captures_allowed && self.captures.get(name).copied().unwrap_or(false)
                    }
                };
            self.context.fields.push(FormatField {
                source: span.start + body_offset + field.start
                    ..span.start + body_offset + field.end,
                runtime_source,
            });
        }
        // Never parse opaque macro input as Rust. This deliberately avoids
        // treating stringify!(format!(...)) or macro_rules! bodies as executed.
    }
}

impl ContextVisitor<'_> {
    fn runtime_value(&self, expression: &Expr) -> bool {
        if runtime_environment(expression) {
            return true;
        }
        let Expr::Path(path) = expression else {
            return false;
        };
        self.captures_allowed
            && path.attrs.is_empty()
            && path.qself.is_none()
            && path.path.get_ident().is_some_and(|name| {
                self.captures
                    .get(&name.to_string())
                    .copied()
                    .unwrap_or(false)
            })
    }
}

// Captured provenance is deliberately narrower than a Rust borrow checker.
// Opaque macro input, unsafe code, mutation and deferred async execution may
// alter or shadow values in ways this source-only analysis cannot establish.
pub(super) struct CaptureHazards(pub(super) bool);

impl<'ast> Visit<'ast> for CaptureHazards {
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let names: Vec<_> = invocation
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if invocation.path.leading_colon.is_none()
            || !matches!(names.as_slice(), [namespace, name]
                if namespace == "std" && ["format", "format_args", "print", "println", "eprint", "eprintln"].contains(&name.as_str())
                || namespace == "core" && name == "format_args")
        {
            self.0 = true;
        }
        // Macro arguments are opaque to Visit. Parse the recognized grammar
        // too, so a mutation/closure/unknown nested macro cannot be hidden in
        // a formatting argument evaluated before the captured field.
        if let Ok(arguments) =
            Punctuated::<Expr, Token![,]>::parse_terminated.parse2(invocation.tokens.clone())
        {
            for argument in arguments {
                if let Expr::Assign(binding) = argument {
                    self.visit_expr(&binding.right);
                } else {
                    self.visit_expr(&argument);
                }
            }
        } else {
            self.0 = true;
        }
    }

    fn visit_expr_unsafe(&mut self, _: &'ast syn::ExprUnsafe) {
        self.0 = true;
    }
    fn visit_expr_assign(&mut self, _: &'ast syn::ExprAssign) {
        self.0 = true;
    }
    fn visit_expr_async(&mut self, _: &'ast syn::ExprAsync) {
        self.0 = true;
    }
    fn visit_expr_reference(&mut self, expression: &'ast syn::ExprReference) {
        if expression.mutability.is_some() {
            self.0 = true;
        }
        syn::visit::visit_expr_reference(self, expression);
    }
    fn visit_expr_binary(&mut self, expression: &'ast syn::ExprBinary) {
        if matches!(
            expression.op,
            syn::BinOp::AddAssign(_)
                | syn::BinOp::SubAssign(_)
                | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_)
                | syn::BinOp::RemAssign(_)
                | syn::BinOp::BitXorAssign(_)
                | syn::BinOp::BitAndAssign(_)
                | syn::BinOp::BitOrAssign(_)
                | syn::BinOp::ShlAssign(_)
                | syn::BinOp::ShrAssign(_)
        ) {
            self.0 = true;
        }
        syn::visit::visit_expr_binary(self, expression);
    }
}

pub(super) struct ArgumentAttributes(pub(super) bool);

impl<'ast> Visit<'ast> for ArgumentAttributes {
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
        self.0 = true;
    }
}

fn ambiguous_namespace_import(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Path(path) => ambiguous_namespace_import(&path.tree),
        syn::UseTree::Name(name) => reserved_namespace(&name.ident) || name.ident == "self",
        syn::UseTree::Rename(rename) => reserved_namespace(&rename.rename),
        syn::UseTree::Glob(_) => true,
        syn::UseTree::Group(group) => group.items.iter().any(ambiguous_namespace_import),
    }
}

fn reserved_namespace(ident: &syn::Ident) -> bool {
    let name = ident.to_string();
    matches!(name.strip_prefix("r#").unwrap_or(&name), "std" | "core")
}

pub(super) fn literal_body(original: &str) -> Option<(&str, usize)> {
    if let Some(body) = original.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        // Decoding escapes would require a verified decoded-to-source map.
        return (!body.contains('\\')).then_some((body, 1));
    }
    let rest = original.strip_prefix('r')?;
    let hashes = rest.bytes().take_while(|byte| *byte == b'#').count();
    let body = rest.get(hashes..)?.strip_prefix('"')?;
    let suffix = format!("\"{}", "#".repeat(hashes));
    Some((body.strip_suffix(&suffix)?, hashes + 2))
}

pub(super) fn replacement_fields(body: &str) -> Option<Vec<(Range<usize>, &str)>> {
    let bytes = body.as_bytes();
    let mut fields = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' if bytes.get(index + 1) == Some(&b'{') => index += 2,
            b'}' if bytes.get(index + 1) == Some(&b'}') => index += 2,
            b'{' => {
                let end = index + body.get(index..)?.find('}')?;
                let name = body.get(index + 1..end)?;
                let mut chars = name.chars();
                if !chars
                    .next()
                    .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
                    || !chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
                {
                    return None;
                }
                fields.push((index..end + 1, name));
                index = end + 1;
            }
            b'}' => return None,
            _ => index += 1,
        }
    }
    Some(fields)
}

fn runtime_environment(expression: &Expr) -> bool {
    runtime_environment_inner(expression, false)
}

fn runtime_environment_inner(expression: &Expr, extracted: bool) -> bool {
    match expression {
        Expr::Paren(expr) if expr.attrs.is_empty() => {
            runtime_environment_inner(&expr.expr, extracted)
        }
        Expr::Group(expr) if expr.attrs.is_empty() => {
            runtime_environment_inner(&expr.expr, extracted)
        }
        Expr::Try(expr) if expr.attrs.is_empty() && !extracted => {
            runtime_environment_inner(&expr.expr, true)
        }
        Expr::MethodCall(expr)
            if expr.method == "unwrap"
                && expr.args.is_empty()
                && expr.turbofish.is_none()
                && expr.attrs.is_empty()
                && !extracted =>
        {
            // After one Result extraction the value is String. A further
            // similarly named method could be an arbitrary extension trait.
            runtime_environment_inner(&expr.receiver, true)
        }
        Expr::Call(call) if call.args.len() == 1 && call.attrs.is_empty() => {
            let Expr::Path(function) = call.func.as_ref() else {
                return false;
            };
            let path = &function.path;
            let names: Vec<_> = path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            path.leading_colon.is_some()
                && function.attrs.is_empty()
                && function.qself.is_none()
                && names == ["std", "env", "var"]
                && path
                    .segments
                    .iter()
                    .all(|segment| segment.arguments.is_empty())
                && matches!(call.args.first(), Some(Expr::Lit(literal)) if literal.attrs.is_empty() && matches!(literal.lit, Lit::Str(_)))
        }
        _ => false,
    }
}

/// A native include's filename cannot override an explicitly constructed shell
/// interpreter. This is a positive boundary finding, never an exemption based
/// on a constructor name or a string's apparent language.
pub(super) fn shell_execution_boundaries(source: &str) -> Vec<Range<usize>> {
    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    struct Commands {
        aliases: std::collections::BTreeSet<String>,
    }
    impl<'ast> Visit<'ast> for Commands {
        fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
            fn imported(tree: &syn::UseTree, aliases: &mut std::collections::BTreeSet<String>) {
                match tree {
                    syn::UseTree::Path(path) => imported(&path.tree, aliases),
                    syn::UseTree::Name(name) if name.ident == "Command" => {
                        aliases.insert("Command".to_owned());
                    }
                    syn::UseTree::Rename(rename) if rename.ident == "Command" => {
                        aliases.insert(rename.rename.to_string());
                    }
                    syn::UseTree::Group(group) => {
                        for item in &group.items {
                            imported(item, aliases);
                        }
                    }
                    _ => {}
                }
            }
            imported(&item.tree, &mut self.aliases);
        }
        fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
            if matches!(item.ty.as_ref(), syn::Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "Command"))
            {
                self.aliases.insert(item.ident.to_string());
            }
        }
    }
    let mut commands = Commands {
        aliases: std::collections::BTreeSet::from(["Command".to_owned()]),
    };
    commands.visit_file(&file);
    struct Boundaries<'a> {
        commands: &'a Commands,
        ranges: Vec<Range<usize>>,
    }
    impl<'ast> Visit<'ast> for Boundaries<'_> {
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if let Expr::Path(function) = call.func.as_ref()
                && function.path.segments.len() >= 2
                && function
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "new")
                && self.commands.aliases.contains(
                    &function.path.segments[function.path.segments.len() - 2]
                        .ident
                        .to_string(),
                )
                && call.args.len() == 1
            {
                let shell_or_unknown = match &call.args[0] {
                    Expr::Lit(literal) => match &literal.lit {
                        Lit::Str(program) => matches!(
                            program.value().rsplit(['/', '\\']).next(),
                            Some(
                                "sh" | "bash"
                                    | "dash"
                                    | "ash"
                                    | "ksh"
                                    | "zsh"
                                    | "cmd"
                                    | "cmd.exe"
                                    | "powershell"
                                    | "pwsh"
                            )
                        ),
                        _ => true,
                    },
                    _ => true,
                };
                if shell_or_unknown {
                    self.ranges.push(call.span().byte_range());
                }
            }
            syn::visit::visit_expr_call(self, call);
        }
        fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
            // Parsing visible expression-shaped input only adds findings. It
            // never asserts an opaque macro executes with standard semantics.
            if let Ok(arguments) =
                Punctuated::<Expr, Token![,]>::parse_terminated.parse2(invocation.tokens.clone())
            {
                for argument in arguments {
                    self.visit_expr(&argument);
                }
            }
        }
    }
    let mut visitor = Boundaries {
        commands: &commands,
        ranges: Vec::new(),
    };
    visitor.visit_file(&file);
    visitor.ranges.sort_by_key(|range| (range.start, range.end));
    visitor.ranges.dedup();
    visitor.ranges
}
