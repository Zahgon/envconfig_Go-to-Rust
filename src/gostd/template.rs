//! The subset of Go's `text/template` that `envconfig`'s usage formats use.
//!
//! Reproduced rather than mapped onto a Rust template engine because the
//! format strings are part of the public API — callers pass their own — and
//! both the rendering and the error text are observable. Supported syntax:
//!
//! - literal text, including single braces such as `{.Key}`, which are *not*
//!   actions;
//! - `{{range .}} … {{end}}` over the gathered variables;
//! - `{{func .}}` for the five usage functions;
//! - `{{.Field}}` for a variable's exported fields.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::VarInfo;

/// A usage function: takes the current variable, returns its rendered text.
pub type UsageFn = fn(&VarInfo) -> Result<String, String>;

/// An error raised while parsing or executing a template, rendered with Go's
/// message layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError(String);

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for TemplateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pos {
    line: usize,
    col: usize,
}

#[derive(Debug, Clone)]
enum Expr {
    /// `.`
    Dot,
    /// `.Name`
    Field(String),
    /// `name .`
    Call(String, Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dot => f.write_str("."),
            Self::Field(name) => write!(f, ".{name}"),
            Self::Call(name, _) => f.write_str(name),
        }
    }
}

#[derive(Debug, Clone)]
enum Node {
    Text(String),
    Action(Expr, Pos),
    Range(Vec<Node>, Pos),
}

/// The value a template action is evaluated against.
enum Value<'a> {
    /// The whole gathered list, at the top level.
    List(&'a [VarInfo]),
    /// One variable, inside a `range` body.
    Item(&'a VarInfo),
}

impl Value<'_> {
    /// The Go type name reported in `can't evaluate field … in type …`.
    fn type_name(&self) -> &'static str {
        match self {
            Self::List(_) => "[]envconfig.VarInfo",
            Self::Item(_) => "envconfig.VarInfo",
        }
    }
}

/// A parsed template.
#[derive(Debug)]
pub struct Template {
    name: String,
    nodes: Vec<Node>,
    funcs: HashMap<String, UsageFn>,
}

/// Locates `offset` within `src` as a 1-based line and the column Go reports
/// for an action, which is the offset of `{{` within its line plus two.
fn position(src: &str, offset: usize) -> Pos {
    let before = &src[..offset];
    let line = before.bytes().filter(|c| *c == b'\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    Pos {
        line,
        col: offset - line_start + 2,
    }
}

impl Template {
    /// Parses `format`, validating that every function it calls is defined.
    pub fn parse(
        name: &str,
        format: &str,
        funcs: HashMap<String, UsageFn>,
    ) -> Result<Self, TemplateError> {
        let mut parser = Parser {
            name,
            src: format,
            pos: 0,
            funcs: &funcs,
        };
        let nodes = parser.parse_nodes(false)?;
        Ok(Self {
            name: name.to_owned(),
            nodes,
            funcs,
        })
    }

    /// The template's name, used in error messages.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Renders the template for `data`.
    pub fn execute(&self, out: &mut dyn fmt::Write, data: &[VarInfo]) -> Result<(), TemplateError> {
        let mut buf = String::new();
        self.walk(&self.nodes, &Value::List(data), &mut buf)?;
        out.write_str(&buf)
            .map_err(|e| TemplateError(format!("template: {}: {e}", self.name)))
    }

    fn walk(&self, nodes: &[Node], dot: &Value<'_>, out: &mut String) -> Result<(), TemplateError> {
        for node in nodes {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Action(expr, pos) => {
                    out.push_str(&self.eval(expr, dot, *pos)?);
                }
                Node::Range(body, pos) => match dot {
                    Value::List(items) => {
                        for item in *items {
                            self.walk(body, &Value::Item(item), out)?;
                        }
                    }
                    Value::Item(_) => {
                        return Err(self.exec_error(
                            *pos,
                            &Expr::Dot,
                            "range can't iterate over a single variable",
                        ));
                    }
                },
            }
        }
        Ok(())
    }

    fn eval(&self, expr: &Expr, dot: &Value<'_>, pos: Pos) -> Result<String, TemplateError> {
        match expr {
            Expr::Dot => Ok(match dot {
                Value::Item(v) => v.key.clone(),
                Value::List(_) => String::new(),
            }),
            Expr::Field(name) => match dot {
                Value::Item(v) => match name.as_str() {
                    "Key" => Ok(v.key.clone()),
                    "Name" => Ok(v.name.to_owned()),
                    "Alt" => Ok(v.alt.to_owned()),
                    _ => Err(self.field_error(pos, expr, name, dot)),
                },
                Value::List(_) => Err(self.field_error(pos, expr, name, dot)),
            },
            Expr::Call(func, arg) => {
                let f = self.funcs.get(func).ok_or_else(|| {
                    self.exec_error(pos, expr, &format!("{func:?} is not a defined function"))
                })?;
                let Expr::Dot = **arg else {
                    return Err(self.exec_error(pos, expr, "expected . as the argument"));
                };
                match dot {
                    Value::Item(v) => f(v).map_err(|e| self.exec_error(pos, expr, &e)),
                    Value::List(_) => Err(self.exec_error(
                        pos,
                        expr,
                        "wrong type for value; expected envconfig.VarInfo",
                    )),
                }
            }
        }
    }

    fn field_error(&self, pos: Pos, expr: &Expr, name: &str, dot: &Value<'_>) -> TemplateError {
        self.exec_error(
            pos,
            expr,
            &format!("can't evaluate field {name} in type {}", dot.type_name()),
        )
    }

    fn exec_error(&self, pos: Pos, expr: &Expr, detail: &str) -> TemplateError {
        TemplateError(format!(
            "template: {}:{}:{}: executing {:?} at <{}>: {}",
            self.name, pos.line, pos.col, self.name, expr, detail
        ))
    }
}

struct Parser<'a> {
    name: &'a str,
    src: &'a str,
    pos: usize,
    funcs: &'a HashMap<String, UsageFn>,
}

impl Parser<'_> {
    fn parse_error(&self, offset: usize, detail: &str) -> TemplateError {
        let line = self.src[..offset].bytes().filter(|c| *c == b'\n').count() + 1;
        TemplateError(format!("template: {}:{}: {}", self.name, line, detail))
    }

    /// Parses nodes until end of input, or until `{{end}}` when `in_range`.
    fn parse_nodes(&mut self, in_range: bool) -> Result<Vec<Node>, TemplateError> {
        let mut nodes = Vec::new();
        let mut text = String::new();
        loop {
            let Some(rel) = self.src[self.pos..].find("{{") else {
                text.push_str(&self.src[self.pos..]);
                self.pos = self.src.len();
                break;
            };
            let open = self.pos + rel;
            text.push_str(&self.src[self.pos..open]);

            let Some(close_rel) = self.src[open + 2..].find("}}") else {
                return Err(self.parse_error(open, "unclosed action"));
            };
            let close = open + 2 + close_rel;
            let body = self.src[open + 2..close].trim();
            self.pos = close + 2;
            let pos = position(self.src, open);

            if body == "end" {
                if !in_range {
                    return Err(self.parse_error(open, "unexpected {{end}}"));
                }
                if !text.is_empty() {
                    nodes.push(Node::Text(std::mem::take(&mut text)));
                }
                return Ok(nodes);
            }

            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }

            if let Some(arg) = body.strip_prefix("range ") {
                if arg.trim() != "." {
                    return Err(self.parse_error(open, "range only supports the . argument"));
                }
                let inner = self.parse_nodes(true)?;
                nodes.push(Node::Range(inner, pos));
                continue;
            }

            nodes.push(Node::Action(self.parse_expr(body, open)?, pos));
        }

        if in_range {
            return Err(self.parse_error(self.src.len(), "unexpected EOF: missing {{end}}"));
        }
        if !text.is_empty() {
            nodes.push(Node::Text(text));
        }
        Ok(nodes)
    }

    fn parse_expr(&self, body: &str, offset: usize) -> Result<Expr, TemplateError> {
        if body.is_empty() {
            return Err(self.parse_error(offset, "missing value for action"));
        }
        if body == "." {
            return Ok(Expr::Dot);
        }
        if let Some(field) = body.strip_prefix('.') {
            if field.contains(char::is_whitespace) {
                return Err(self.parse_error(offset, "unsupported field expression"));
            }
            return Ok(Expr::Field(field.to_owned()));
        }
        // `func .`
        let mut parts = body.split_whitespace();
        let func = parts.next().unwrap_or_default().to_owned();
        let arg = parts.next().unwrap_or_default();
        if parts.next().is_some() {
            return Err(self.parse_error(offset, "too many arguments"));
        }
        if !self.funcs.contains_key(&func) {
            return Err(self.parse_error(offset, &format!("function {func:?} not defined")));
        }
        if arg != "." {
            return Err(self.parse_error(offset, "expected . as the argument"));
        }
        Ok(Expr::Call(func, Box::new(Expr::Dot)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(key: &str) -> VarInfo {
        VarInfo {
            name: "Field",
            alt: "",
            key: key.to_owned(),
            type_desc: "String",
            default: "",
            required: "",
            desc: "d",
        }
    }

    fn funcs() -> HashMap<String, UsageFn> {
        let mut m: HashMap<String, UsageFn> = HashMap::new();
        m.insert("usage_key".to_owned(), |v| Ok(v.key.clone()));
        m.insert("usage_description".to_owned(), |v| Ok(v.desc.to_owned()));
        m
    }

    fn render(format: &str) -> Result<String, TemplateError> {
        let t = Template::parse("envconfig", format, funcs())?;
        let mut out = String::new();
        t.execute(&mut out, &[info("A"), info("B")])?;
        Ok(out)
    }

    #[test]
    fn ranges_and_calls_functions() {
        assert_eq!(
            render("{{range .}}{{usage_key .}}={{usage_description .}}\n{{end}}").unwrap(),
            "A=d\nB=d\n"
        );
    }

    #[test]
    fn single_braces_are_literal_text() {
        assert_eq!(
            render("{{range .}}{.Key}\n{{end}}").unwrap(),
            "{.Key}\n{.Key}\n"
        );
    }

    /// The exact prefix the Go suite asserts on.
    #[test]
    fn unknown_field_error_matches_go() {
        let err = render("{{.UnknownKey}}").unwrap_err();
        assert!(
            err.to_string()
                .contains("template: envconfig:1:2: executing \"envconfig\" at <.UnknownKey>"),
            "got {err}"
        );
    }

    /// Go reports the column of `{{` plus two.
    #[test]
    fn error_positions_match_go() {
        for (format, want) in [
            ("{{.UnknownKey}}", "envconfig:1:2:"),
            ("x{{.UnknownKey}}", "envconfig:1:3:"),
            ("xy{{.UnknownKey}}", "envconfig:1:4:"),
            ("\n{{.UnknownKey}}", "envconfig:2:2:"),
            ("{{range .}}{{.Nope}}{{end}}", "envconfig:1:13:"),
        ] {
            let err = render(format).unwrap_err().to_string();
            assert!(err.contains(want), "format {format:?} gave {err}");
        }
    }

    #[test]
    fn known_fields_resolve() {
        assert_eq!(render("{{range .}}{{.Key}},{{end}}").unwrap(), "A,B,");
    }

    #[test]
    fn undefined_function_fails_at_parse_time() {
        let err = Template::parse("envconfig", "{{nope .}}", funcs()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "template: envconfig:1: function \"nope\" not defined"
        );
    }

    #[test]
    fn unbalanced_actions_fail_at_parse_time() {
        assert!(Template::parse("envconfig", "{{range .}}x", funcs()).is_err());
        assert!(Template::parse("envconfig", "{{end}}", funcs()).is_err());
        assert!(Template::parse("envconfig", "{{range .}", funcs()).is_err());
    }
    #[test]
    fn the_template_reports_its_name() {
        let t = Template::parse("envconfig", "x", funcs()).unwrap();
        assert_eq!(t.name(), "envconfig");
    }

    #[test]
    fn bare_dot_renders_the_key_inside_a_range() {
        assert_eq!(render("{{range .}}{{.}};{{end}}").unwrap(), "A;B;");
    }

    #[test]
    fn bare_dot_at_the_top_level_renders_nothing() {
        assert_eq!(render("{{.}}").unwrap(), "");
    }

    #[test]
    fn a_function_applied_outside_a_range_is_an_error() {
        let err = render("{{usage_key .}}").unwrap_err().to_string();
        assert!(err.contains("wrong type for value"), "{err}");
    }

    #[test]
    fn a_nested_range_over_a_single_variable_is_an_error() {
        let err = render("{{range .}}{{range .}}x{{end}}{{end}}")
            .unwrap_err()
            .to_string();
        assert!(err.contains("range can't iterate"), "{err}");
    }

    #[test]
    fn a_failing_function_surfaces_its_message() {
        let mut m: HashMap<String, UsageFn> = HashMap::new();
        m.insert("boom".to_owned(), |_| Err("kaboom".to_owned()));
        let t = Template::parse("envconfig", "{{range .}}{{boom .}}{{end}}", m).unwrap();
        let mut out = String::new();
        let err = t.execute(&mut out, &[info("A")]).unwrap_err().to_string();
        assert!(err.contains("kaboom"), "{err}");
    }

    #[test]
    fn malformed_actions_are_rejected_at_parse_time() {
        for format in [
            "{{}}",
            "{{range .}}{{end}}{{end}}",
            "{{usage_key . .}}",
            "{{usage_key x}}",
            "{{range x}}{{end}}",
            "{{.Some Field}}",
        ] {
            assert!(
                Template::parse("envconfig", format, funcs()).is_err(),
                "expected {format:?} to fail"
            );
        }
    }

    #[test]
    fn an_unknown_field_inside_a_range_names_the_item_type() {
        let err = render("{{range .}}{{.Nope}}{{end}}")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("can't evaluate field Nope in type envconfig.VarInfo"),
            "{err}"
        );
    }

    #[test]
    fn an_unknown_field_at_the_top_level_names_the_list_type() {
        let err = render("{{.Nope}}").unwrap_err().to_string();
        assert!(
            err.contains("can't evaluate field Nope in type []envconfig.VarInfo"),
            "{err}"
        );
    }

    #[test]
    fn expressions_render_themselves_in_errors() {
        assert_eq!(Expr::Dot.to_string(), ".");
        assert_eq!(Expr::Field("X".to_owned()).to_string(), ".X");
        assert_eq!(
            Expr::Call("usage_key".to_owned(), Box::new(Expr::Dot)).to_string(),
            "usage_key"
        );
    }

    #[test]
    fn name_and_alt_fields_resolve() {
        assert_eq!(
            render("{{range .}}{{.Name}}{{.Alt}};{{end}}").unwrap(),
            "Field;Field;"
        );
    }
}
