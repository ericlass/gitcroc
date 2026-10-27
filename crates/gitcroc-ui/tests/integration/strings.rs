//! D15: the UI's text comes from `src/strings.rs`. As `ui/src/strings/
//! strings.test.ts` does for the web front end, this test parses every
//! source file and reports UI text written inline: string literals with a
//! letter in them that reach the user through a text method or parameter
//! (`child`, `aria_label`, `label`, a menu item's name, …) or a text field
//! (`title`, `name`, …). Text that is data (ids, key names, paths, what the
//! core sends) never reaches those, so it is not seen here.

use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, FieldValue, ItemMod, Lit, Member};

/// Methods whose arguments reach the user.
const TEXT_METHODS: [&str; 10] = [
    "child",
    "children",
    "label",
    "aria_label",
    "tooltip",
    "title",
    "placeholder",
    "prompt",
    "set_title",
    "description",
];

/// Functions, by the last part of their path, whose arguments reach the
/// user: a menu item.
const TEXT_FUNCTIONS: [&str; 2] = ["action", "os_action"];

/// Types whose `new` takes text: a menu, a label.
const TEXT_CONSTRUCTORS: [&str; 2] = ["Menu", "Label"];

/// Struct fields whose value reaches the user.
const TEXT_FIELDS: [&str; 5] = ["title", "name", "label", "prompt", "message"];

/// Whether `text` is UI text: it has a letter, other than in the product
/// name.
fn has_words(text: &str) -> bool {
    text.replace("gitcroc", "").chars().any(char::is_alphabetic)
}

struct Scan {
    /// Each text found, with its line.
    found: Vec<(usize, String)>,
}

impl Scan {
    fn report(&mut self, text: &str, line: usize) {
        if has_words(text) {
            self.found.push((line, text.to_owned()));
        }
    }

    /// The literals `expr` evaluates to, not those it only compares or
    /// passes on.
    fn results(&mut self, expr: &Expr) {
        match expr {
            Expr::Lit(lit) => {
                if let Lit::Str(text) = &lit.lit {
                    self.report(&text.value(), text.span().start().line);
                }
            }
            // `format!` and the like: every literal in it is text.
            Expr::Macro(mac) => {
                for token in mac.mac.tokens.clone() {
                    if let proc_macro2::TokenTree::Literal(lit) = token
                        && let Ok(Lit::Str(text)) = syn::parse_str::<Lit>(&lit.to_string())
                    {
                        self.report(&text.value(), lit.span().start().line);
                    }
                }
            }
            Expr::Paren(inner) => self.results(&inner.expr),
            Expr::Reference(inner) => self.results(&inner.expr),
            Expr::Group(inner) => self.results(&inner.expr),
            Expr::If(branches) => {
                if let Some(tail) = branches.then_branch.stmts.last() {
                    self.statement(tail);
                }
                if let Some((_, other)) = &branches.else_branch {
                    self.results(other);
                }
            }
            Expr::Block(block) => {
                if let Some(tail) = block.block.stmts.last() {
                    self.statement(tail);
                }
            }
            Expr::Match(arms) => {
                for arm in &arms.arms {
                    self.results(&arm.body);
                }
            }
            Expr::Array(items) => items.elems.iter().for_each(|item| self.results(item)),
            // `"text".into()`, `"text".to_owned()`.
            Expr::MethodCall(call) if call.args.is_empty() => self.results(&call.receiver),
            // `Some("text")`, `SharedString::from("text")`.
            Expr::Call(call)
                if path_parts(&call.func)
                    .is_some_and(|(_, last)| last == "Some" || last == "from") =>
            {
                call.args.iter().for_each(|arg| self.results(arg));
            }
            _ => {}
        }
    }

    fn statement(&mut self, statement: &syn::Stmt) {
        if let syn::Stmt::Expr(expr, None) = statement {
            self.results(expr);
        }
    }
}

/// The last part of `func`'s path, and the part before it.
fn path_parts(func: &Expr) -> Option<(Option<String>, String)> {
    let Expr::Path(path) = func else {
        return None;
    };
    let segments: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    let (last, rest) = segments.split_last()?;
    Some((rest.last().cloned(), last.clone()))
}

/// Whether `attrs` mark test code.
fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string().contains("test")))
    })
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if !is_test(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !is_test(&item.attrs) {
            visit::visit_item_fn(self, item);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if TEXT_METHODS.iter().any(|name| call.method == name) {
            call.args.iter().for_each(|arg| self.results(arg));
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Some((owner, name)) = path_parts(&call.func) {
            let text = match name.as_str() {
                "new" => owner.is_some_and(|owner| TEXT_CONSTRUCTORS.contains(&owner.as_str())),
                name => TEXT_FUNCTIONS.contains(&name),
            };
            if text {
                call.args.iter().for_each(|arg| self.results(arg));
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_field_value(&mut self, field: &'ast FieldValue) {
        if let Member::Named(name) = &field.member
            && TEXT_FIELDS.iter().any(|text| name == text)
        {
            self.results(&field.expr);
        }
        visit::visit_field_value(self, field);
    }
}

/// The inline UI text of `code`, a file named `file`, in line order.
fn scan(file: &str, code: &str) -> Vec<String> {
    let parsed = syn::parse_file(code).unwrap_or_else(|error| panic!("{file}: {error}"));
    let mut scan = Scan { found: Vec::new() };
    scan.visit_file(&parsed);
    // A chain of calls is visited from its last call back.
    scan.found.sort_by_key(|(line, _)| *line);
    scan.found
        .into_iter()
        .map(|(line, text)| format!("{file}:{line} {text:?}"))
        .collect()
}

/// The UI's source files: everything in `src` but the strings themselves.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "strings.rs")
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn d15_ui_text_comes_from_the_strings_module() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = sources(&src);
    assert!(files.len() > 5, "{files:?}");
    let found: Vec<String> = files
        .iter()
        .flat_map(|path| {
            let name = path.strip_prefix(&src).unwrap().display().to_string();
            scan(&name, &std::fs::read_to_string(path).unwrap())
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn d15_the_scan_finds_inline_text_and_lets_data_through() {
    let code = r#"
        fn view(name: &str, count: usize, wide: bool) {
            let menu = Menu::new("Edit").items([
                MenuItem::action("Undo", Undo),
                MenuItem::action(strings::menus::REDO, Redo),
            ]);
            div()
                .id("pane")
                .key_context("Pane")
                .child("Inline text")
                .child(if count > 1 { "Several" } else { name })
                .child(format!("Delete {name}"))
                .child(strings::regions::TREE)
                .aria_label(match wide { true => "Wide", false => strings::APP_NAME })
                .child(SharedString::from("Shared"))
                .tooltip("Tip".into())
                .child(svg().path("icons/close.svg"))
                .child(name.to_owned())
                .child("gitcroc …");
            let options = TitlebarOptions { title: Some("Title".into()), ..Default::default() };
            KeyBinding::new("cmd-q", Quit, None);
            log::warn!("not shown to the user");
        }

        #[cfg(test)]
        mod tests {
            fn sample() { div().child("Test text"); }
        }
    "#;
    let found: Vec<String> = scan("example.rs", code)
        .into_iter()
        .map(|found| found.split_once(' ').unwrap().1.to_owned())
        .collect();
    assert_eq!(
        found,
        [
            "\"Edit\"",
            "\"Undo\"",
            "\"Inline text\"",
            "\"Several\"",
            "\"Delete {name}\"",
            "\"Wide\"",
            "\"Shared\"",
            "\"Tip\"",
            "\"Title\"",
        ]
    );
}
