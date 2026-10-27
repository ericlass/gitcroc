//! UI-23: app colours exist only in `src/theme/tokens.rs`, code theme
//! colours only in `src/theme/code_themes.rs` (UI-24). As rc.1's
//! `ui/src/theme/colors.test.ts` did for the web front end, this parses
//! every other source file and reports a colour written in it: a call to
//! one of gpui's colour constructors or named colours, a `Color`, `Hsla`
//! or `Rgba` built by hand, or a `#rrggbb`-shaped string.

use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprStruct, ItemMod, Lit, LitStr, Macro};

/// The files allowed to hold colours.
const ALLOWED: [&str; 2] = ["theme/tokens.rs", "theme/code_themes.rs"];

/// gpui's colour constructors and named colours, and the tokens' `Color`.
/// `transparent_black` and `transparent_white` carry no colour and are
/// allowed, as `transparent` was in CSS.
const COLOR_FUNCTIONS: [&str; 11] = [
    "rgb",
    "rgba",
    "hsla",
    "opaque_grey",
    "black",
    "white",
    "red",
    "green",
    "blue",
    "yellow",
    "Color",
];

/// Types whose literals are colours.
const COLOR_TYPES: [&str; 3] = ["Hsla", "Rgba", "Color"];

struct Scan {
    found: Vec<(usize, String)>,
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

/// Whether `text` holds a hex colour: `#` and 3, 4, 6 or 8 hex digits, not
/// followed by another letter or digit.
fn has_hex_color(text: &str) -> bool {
    text.match_indices('#').any(|(at, _)| {
        let rest = &text[at + 1..];
        let digits = rest.chars().take_while(char::is_ascii_hexdigit).count();
        let next = rest[digits..].chars().next();
        [3, 4, 6, 8].contains(&digits) && !next.is_some_and(char::is_alphanumeric)
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

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(path) = &*call.func
            && let Some(last) = path.path.segments.last()
            && COLOR_FUNCTIONS.iter().any(|name| last.ident == name)
        {
            let line = last.ident.span().start().line;
            self.found.push((line, format!("{}(..)", last.ident)));
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_expr_struct(&mut self, item: &'ast ExprStruct) {
        if let Some(last) = item.path.segments.last()
            && COLOR_TYPES.iter().any(|name| last.ident == name)
        {
            let line = last.ident.span().start().line;
            self.found.push((line, format!("{} {{ .. }}", last.ident)));
        }
        visit::visit_expr_struct(self, item);
    }

    fn visit_lit_str(&mut self, text: &'ast LitStr) {
        self.string(text);
    }

    /// `json!`, `format!` and the like: the strings among their tokens.
    fn visit_macro(&mut self, mac: &'ast Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

impl Scan {
    fn string(&mut self, text: &LitStr) {
        if has_hex_color(&text.value()) {
            self.found
                .push((text.span().start().line, format!("{:?}", text.value())));
        }
    }

    fn tokens(&mut self, tokens: proc_macro2::TokenStream) {
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                proc_macro2::TokenTree::Literal(literal) => {
                    if let Ok(Lit::Str(text)) = syn::parse_str::<Lit>(&literal.to_string()) {
                        let text = LitStr::new(&text.value(), literal.span());
                        self.string(&text);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The colours written in `code`, a file named `file`, in line order.
fn scan(file: &str, code: &str) -> Vec<String> {
    let parsed = syn::parse_file(code).unwrap_or_else(|error| panic!("{file}: {error}"));
    let mut scan = Scan { found: Vec::new() };
    scan.visit_file(&parsed);
    scan.found.sort_by_key(|(line, _)| *line);
    scan.found
        .into_iter()
        .map(|(line, found)| format!("{file}:{line} {found}"))
        .collect()
}

fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn ui_23_no_hardcoded_colors() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut allowed = Vec::new();
    for path in sources(&src) {
        let name = path
            .strip_prefix(&src)
            .unwrap()
            .display()
            .to_string()
            .replace('\\', "/");
        let colors = scan(&name, &std::fs::read_to_string(&path).unwrap());
        if ALLOWED.contains(&name.as_str()) {
            allowed.push((name, colors.len()));
        } else {
            found.extend(colors);
        }
    }
    assert_eq!(
        found,
        Vec::<String>::new(),
        "use a token from src/theme/tokens.rs or a colour of src/theme/code_themes.rs"
    );
    // The allowed files themselves hold the colours they promise.
    assert_eq!(allowed.len(), ALLOWED.len(), "{allowed:?}");
    assert!(allowed.iter().all(|(_, count)| *count > 10), "{allowed:?}");
}

#[test]
fn ui_23_the_color_scan_catches_every_kind_of_literal() {
    let code = r##"
        fn view(tokens: &Tokens, theme: &Theme) {
            div()
                .bg(rgb(0xff0000))
                .border_color(gpui::rgba(0x00000080))
                .text_color(hsla(0.5, 0.5, 0.5, 1.))
                .bg(gpui::black())
                .text_color(white())
                .bg(Color(0x123456))
                .text_color(Hsla { h: 0., s: 0., l: 0., a: 1. })
                .bg(Rgba { r: 1., g: 0., b: 0., a: 1. })
                .child(try_parse_color("#abc").unwrap())
                .child(serde_json::json!({ "background": "#ff00ff80" }));
            div()
                .bg(tokens.bg_1)
                .text_color(theme.foreground)
                .bg(gpui::transparent_black())
                .child("#abcdef0123 and #ghi")
                .child(tokens.accent.hex());
        }

        #[cfg(test)]
        mod tests {
            fn sample() { div().bg(rgb(0x000000)); }
        }
    "##;
    let found: Vec<String> = scan("example.rs", code)
        .into_iter()
        .map(|found| found.split_once(' ').unwrap().1.to_owned())
        .collect();
    assert_eq!(
        found,
        [
            "rgb(..)",
            "rgba(..)",
            "hsla(..)",
            "black(..)",
            "white(..)",
            "Color(..)",
            "Hsla { .. }",
            "Rgba { .. }",
            "\"#abc\"",
            "\"#ff00ff80\"",
        ]
    );
}
