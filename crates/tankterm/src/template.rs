//! Format strings with WezTerm-like `{name}` placeholders, for the tab bar.
//!
//! `{{` and `}}` stand for literal braces. A `{` without a closing `}` is kept as text.

use std::borrow::Cow;

/// A parsed format string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Template {
    pieces: Vec<Piece>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    Field(String),
}

impl Template {
    pub fn parse(format: &str) -> Self {
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut rest = format;

        while let Some(position) = rest.find(['{', '}']) {
            text.push_str(&rest[..position]);
            let brace = &rest[position..];

            if brace.starts_with("{{") || brace.starts_with("}}") {
                text.push_str(&brace[..1]);
                rest = &brace[2..];
            } else if let Some(end) = brace.strip_prefix('{').and_then(|field| field.find('}')) {
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Field(brace[1..=end].to_owned()));
                rest = &brace[end + 2..];
            } else {
                text.push_str(&brace[..1]);
                rest = &brace[1..];
            }
        }

        text.push_str(rest);
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }

        Self { pieces }
    }

    /// Names of the placeholders, in order.
    pub fn fields(&self) -> impl Iterator<Item = &str> {
        self.pieces.iter().filter_map(|piece| match piece {
            Piece::Field(name) => Some(name.as_str()),
            Piece::Text(_) => None,
        })
    }

    /// The text with every placeholder replaced by its value, or `None` if a placeholder has
    /// no value.
    pub fn render<'a>(&self, value: impl Fn(&str) -> Option<Cow<'a, str>>) -> Option<String> {
        let mut rendered = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(text) => rendered.push_str(text),
                Piece::Field(name) => rendered.push_str(&value(name)?),
            }
        }
        Some(rendered)
    }

    /// The text like [`Template::render`] gives it, in parts: `true` for text that the
    /// placeholder `field` gave, `false` for the rest. Parts next to each other are joined, and
    /// empty ones left out.
    pub fn render_parts<'a>(
        &self,
        field: &str,
        value: impl Fn(&str) -> Option<Cow<'a, str>>,
    ) -> Option<Vec<(String, bool)>> {
        let mut parts: Vec<(String, bool)> = Vec::new();
        for piece in &self.pieces {
            let (text, from_field) = match piece {
                Piece::Text(text) => (Cow::Borrowed(text.as_str()), false),
                Piece::Field(name) => (value(name)?, name == field),
            };
            if text.is_empty() {
                continue;
            }
            match parts.last_mut() {
                Some((last, kind)) if *kind == from_field => last.push_str(&text),
                _ => parts.push((text.into_owned(), from_field)),
            }
        }
        Some(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(format: &str) -> Option<String> {
        Template::parse(format).render(|name| match name {
            "title" => Some("vim".into()),
            "index" => Some("2".into()),
            _ => None,
        })
    }

    #[test]
    fn parts_say_which_text_a_field_produced() {
        let value = |name: &str| match name {
            "index" => Some("2".into()),
            "title" => Some("vim".into()),
            _ => None,
        };
        let parts = Template::parse("{index}  {title}!").render_parts("index", value);
        assert_eq!(parts.unwrap(), [("2".into(), true), ("  vim!".into(), false)]);

        // Pieces next to each other are joined, and a placeholder without a value is `None`.
        let parts = Template::parse("[{index}{index}]").render_parts("index", value);
        assert_eq!(parts.unwrap(), [("[".into(), false), ("22".into(), true), ("]".into(), false)]);
        assert_eq!(Template::parse("{nope}").render_parts("index", value), None);
    }

    #[test]
    fn placeholders_are_replaced() {
        assert_eq!(render("{index}: {title}!").as_deref(), Some("2: vim!"));
        assert_eq!(render("{title}{title}").as_deref(), Some("vimvim"));
        assert_eq!(render("plain").as_deref(), Some("plain"));
        assert_eq!(render("").as_deref(), Some(""));
    }

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(render("{{title}} {{{title}}}").as_deref(), Some("{title} {vim}"));
    }

    #[test]
    fn unclosed_braces_are_text() {
        assert_eq!(render("a { b").as_deref(), Some("a { b"));
        assert_eq!(render("a } b").as_deref(), Some("a } b"));
        assert_eq!(render("{title").as_deref(), Some("{title"));
    }

    #[test]
    fn missing_values_render_nothing() {
        assert_eq!(render("{index} {nothing}"), None);
    }

    #[test]
    fn fields_are_listed() {
        let template = Template::parse("{a} {{b}} {c}");
        assert_eq!(template.fields().collect::<Vec<_>>(), ["a", "c"]);
    }
}
