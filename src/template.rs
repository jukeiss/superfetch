//! The format strings used by every module.
//!
//! ```text
//! {name}        value of a field; empty when the field is unavailable
//! {#spec}       switch style, e.g. {#bold red} or {#accent}; {#} resets
//! {? ... }      optional group, dropped when a field inside it is unavailable
//! {{  }}        literal braces (outside optional groups)
//! ```
//!
//! A template is parsed once, before the module runs, so the module can ask
//! which fields are wanted and skip the work for the rest.

use crate::style::{RESET, Style};

#[derive(Debug, PartialEq)]
enum Token {
    Text(String),
    Field(String),
    Style(String),
    Group(Vec<Token>),
}

#[derive(Debug, PartialEq)]
pub struct Template(Vec<Token>);

/// Supplies the values a template refers to.
pub trait Context {
    fn field(&self, name: &str) -> Option<String>;
    fn style(&self, spec: &str) -> Style;
}

impl Template {
    /// Parsing never fails: a brace that does not open a valid tag is kept
    /// as literal text so a typo shows up in the output instead of hiding it.
    pub fn parse(source: &str) -> Template {
        let mut rest = source;
        Template(parse_tokens(&mut rest, false))
    }

    /// Whether `{name}` appears anywhere in the template.
    pub fn wants(&self, name: &str) -> bool {
        fn search(tokens: &[Token], name: &str) -> bool {
            tokens.iter().any(|token| match token {
                Token::Field(field) => field == name,
                Token::Group(inner) => search(inner, name),
                _ => false,
            })
        }
        search(&self.0, name)
    }

    pub fn render(&self, ctx: &dyn Context) -> String {
        let mut out = String::new();
        let mut active = String::new();
        render_tokens(&self.0, ctx, &mut out, &mut active);
        if !active.is_empty() {
            out.push_str(RESET);
        }
        out
    }
}

fn parse_tokens(rest: &mut &str, in_group: bool) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, tokens: &mut Vec<Token>| {
        if !text.is_empty() {
            tokens.push(Token::Text(std::mem::take(text)));
        }
    };

    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix("{{") {
            text.push('{');
            *rest = tail;
        } else if c == '}' && in_group {
            // Checked before the `}}` escape so nested groups can close
            // back to back.
            *rest = &rest[1..];
            flush(&mut text, &mut tokens);
            return tokens;
        } else if let Some(tail) = rest.strip_prefix("}}") {
            text.push('}');
            *rest = tail;
        } else if let Some(tail) = rest.strip_prefix("{?") {
            flush(&mut text, &mut tokens);
            *rest = tail;
            tokens.push(Token::Group(parse_tokens(rest, true)));
        } else if let Some((tag, tail)) = rest.strip_prefix('{').and_then(|r| r.split_once('}'))
            && !tag.contains('{')
        {
            flush(&mut text, &mut tokens);
            *rest = tail;
            tokens.push(match tag.strip_prefix('#') {
                Some(spec) => Token::Style(spec.trim().to_string()),
                None => Token::Field(tag.trim().to_string()),
            });
        } else {
            text.push(c);
            *rest = &rest[c.len_utf8()..];
        }
    }
    flush(&mut text, &mut tokens);
    tokens
}

/// Renders `tokens` into `out` and reports whether every field had a value.
/// `active` holds the escape sequence of the style currently switched on.
fn render_tokens(
    tokens: &[Token],
    ctx: &dyn Context,
    out: &mut String,
    active: &mut String,
) -> bool {
    let mut complete = true;
    for token in tokens {
        match token {
            Token::Text(text) => out.push_str(text),
            Token::Field(name) => match ctx.field(name).filter(|v| !v.is_empty()) {
                Some(value) => {
                    out.push_str(&value);
                    // A value that brings its own colors ends with a reset,
                    // which would also cancel the style around it.
                    if value.contains('\x1b') {
                        out.push_str(active);
                    }
                }
                None => complete = false,
            },
            Token::Style(spec) => {
                if !active.is_empty() {
                    out.push_str(RESET);
                }
                *active = if spec.is_empty() {
                    String::new()
                } else {
                    ctx.style(spec).sgr()
                };
                out.push_str(active);
            }
            Token::Group(inner) => {
                let (mark, saved) = (out.len(), active.clone());
                if !render_tokens(inner, ctx, out, active) {
                    out.truncate(mark);
                    *active = saved;
                }
            }
        }
    }
    complete
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    struct Fixture;

    impl Context for Fixture {
        fn field(&self, name: &str) -> Option<String> {
            match name {
                "model" => Some("Ryzen 7".into()),
                "cores" => Some("16".into()),
                "percent" => Some("\x1b[31m81%\x1b[0m".into()),
                "blank" => Some(String::new()),
                _ => None,
            }
        }

        fn style(&self, spec: &str) -> Style {
            Style::parse(spec, Some(Color::Ansi(4))).unwrap_or_default()
        }
    }

    fn render(source: &str) -> String {
        Template::parse(source).render(&Fixture)
    }

    #[test]
    fn substitutes_fields() {
        assert_eq!(render("{model} ({cores})"), "Ryzen 7 (16)");
        assert_eq!(render("{ model }"), "Ryzen 7");
        assert_eq!(render("[{missing}]"), "[]");
    }

    #[test]
    fn drops_groups_with_missing_fields() {
        assert_eq!(render("{model}{? @ {freq}}"), "Ryzen 7");
        assert_eq!(render("{model}{? ({cores})}"), "Ryzen 7 (16)");
        assert_eq!(render("{model}{? [{blank}]}"), "Ryzen 7");
        assert_eq!(render("a{? b{? {freq}} {cores}}"), "a b 16");
    }

    #[test]
    fn keeps_literal_braces() {
        assert_eq!(render("{{model}}"), "{model}");
        assert_eq!(render("open { brace"), "open { brace");
        assert_eq!(render("{? {{{cores}}"), " {16");
        assert_eq!(render("{? a{? {cores}}}"), " a 16");
    }

    #[test]
    fn applies_and_resets_styles() {
        assert_eq!(render("{#red}x{#}y"), "\x1b[31mx\x1b[0my");
        assert_eq!(render("{#bold accent}x"), "\x1b[1;34mx\x1b[0m");
        assert_eq!(
            render("{#red}a{#green}b"),
            "\x1b[31ma\x1b[0m\x1b[32mb\x1b[0m"
        );
    }

    #[test]
    fn restores_style_after_colored_field() {
        assert_eq!(
            render("{#dim}{percent} used"),
            "\x1b[2m\x1b[31m81%\x1b[0m\x1b[2m used\x1b[0m"
        );
    }

    #[test]
    fn dropped_group_leaves_style_untouched() {
        assert_eq!(render("{#red}a{? {#green}{freq}}b"), "\x1b[31mab\x1b[0m");
    }

    #[test]
    fn reports_wanted_fields() {
        let template = Template::parse("{model}{? · {usage}}");
        assert!(template.wants("model"));
        assert!(template.wants("usage"));
        assert!(!template.wants("freq"));
    }
}
