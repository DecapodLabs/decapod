//! Literal text consumed only by primitive standard-library boolean operations.
//! No method-name inference or local-variable/return flow is performed.

use std::ops::Range;
use std::path::Path;
use syn::visit::Visit;
use syn::{Expr, Lit};

pub(super) fn ranges(root: &Path, path: &Path, source: &str) -> Vec<Range<usize>> {
    if !super::source_dependencies::modern_crate_root(root, path)
        || source.starts_with('\u{feff}')
        || (source.starts_with("#!") && !source.starts_with("#!["))
    {
        return Vec::new();
    }
    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    if !super::runtime_credentials::standard_namespace_is_unambiguous(&file) {
        return Vec::new();
    }
    let mut visitor = TextVisitor {
        source,
        ranges: Vec::new(),
    };
    visitor.visit_file(&file);
    visitor.ranges
}

struct TextVisitor<'a> {
    source: &'a str,
    ranges: Vec<Range<usize>>,
}

fn no_rewrite(attributes: &[syn::Attribute], body: &syn::Block) -> bool {
    let mut nested = super::rust_context::ArgumentAttributes(false);
    nested.visit_block(body);
    attributes.iter().all(super::runtime_credentials::inert) && !nested.0
}

impl<'ast> Visit<'ast> for TextVisitor<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        // Enter only scopes whose complete ancestor contract is checked here.
        // Trait/const/static bodies and their procedural attributes are not
        // part of this bounded boolean-sink recognizer.
        match item {
            syn::Item::Fn(function) => self.visit_item_fn(function),
            syn::Item::Impl(implementation) => self.visit_item_impl(implementation),
            syn::Item::Mod(module) => self.visit_item_mod(module),
            _ => {}
        }
    }
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if let syn::ImplItem::Fn(function) = item {
            self.visit_impl_item_fn(function);
        }
    }
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if no_rewrite(&function.attrs, &function.block) && function.sig.unsafety.is_none() {
            syn::visit::visit_item_fn(self, function);
        }
    }
    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        if no_rewrite(&function.attrs, &function.block) && function.sig.unsafety.is_none() {
            syn::visit::visit_impl_item_fn(self, function);
        }
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if item.attrs.iter().all(super::runtime_credentials::inert) {
            syn::visit::visit_item_impl(self, item);
        }
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.attrs.iter().all(super::runtime_credentials::inert) {
            syn::visit::visit_item_mod(self, item);
        }
    }
    fn visit_macro(&mut self, _: &'ast syn::Macro) {}
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if call.attrs.is_empty()
            && call.args.len() == 2
            && let Expr::Path(function) = call.func.as_ref()
            && function.attrs.is_empty()
            && primitive_boolean(function)
            && let Expr::Lit(literal) = &call.args[1]
            && literal.attrs.is_empty()
            && let Lit::Str(literal) = &literal.lit
        {
            let range = literal.token().span().byte_range();
            if self.source.get(range.clone()) == Some(literal.token().to_string().as_str()) {
                self.ranges.push(range);
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
}

fn exact(path: &syn::Path, names: &[&str]) -> bool {
    path.leading_colon.is_some()
        && path.segments.len() == names.len()
        && path
            .segments
            .iter()
            .zip(names)
            .all(|(segment, name)| segment.ident == *name && segment.arguments.is_empty())
}

fn primitive_boolean(function: &syn::ExprPath) -> bool {
    if function.qself.is_none() {
        return exact(&function.path, &["std", "primitive", "str", "contains"]);
    }
    let Some(qself) = &function.qself else {
        return false;
    };
    let syn::Type::Path(ty) = qself.ty.as_ref() else {
        return false;
    };
    qself.as_token.is_some()
        && qself.position == 3
        && ty.qself.is_none()
        && exact(&ty.path, &["std", "primitive", "str"])
        && exact(&function.path, &["std", "cmp", "PartialEq", "eq"])
}
