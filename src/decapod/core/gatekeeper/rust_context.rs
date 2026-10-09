//! Conservative source evidence for Rust formatting expressions.
//!
//! This is not macro expansion or whole-program dataflow. Unknown syntax and
//! sources retain findings. In particular, a replacement field alone is not
//! proof that the supplied credential was not hardcoded elsewhere.

use std::collections::BTreeMap;
use std::ops::Range;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Expr, Lit, Token};

#[derive(Default)]
pub(super) struct RustContext {
    fields: Vec<FormatField>,
    uncertain_expansion: bool,
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
        };
        visitor.visit_file(&file);
        visitor.context
    }

    pub(super) fn password_value(&self, source: Range<usize>) -> PasswordContext {
        if self.uncertain_expansion {
            return PasswordContext::LiteralOrUnknown;
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
}

impl<'ast> Visit<'ast> for ContextVisitor<'_> {
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
                && bindings
                    .get(name)
                    .is_some_and(|value| runtime_environment(value));
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

struct ArgumentAttributes(bool);

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

fn literal_body(original: &str) -> Option<(&str, usize)> {
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

fn replacement_fields(body: &str) -> Option<Vec<(Range<usize>, &str)>> {
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
