//! Usage rendering.

use std::collections::HashMap;
use std::io::Write;

use crate::gostd::strconv;
use crate::gostd::tabwriter::TabWriter;
use crate::gostd::template::{Template, UsageFn};
use crate::{gather_info, EnvConfig, Error, VarInfo};

/// Format string that displays usage in a list format.
pub const DEFAULT_LIST_FORMAT: &str = "\
This application is configured via the environment. The following environment
variables can be used:
{{range .}}
{{usage_key .}}
  [description] {{usage_description .}}
  [type]        {{usage_type .}}
  [default]     {{usage_default .}}
  [required]    {{usage_required .}}{{end}}
";

/// Format string that displays usage in a tabular format.
///
/// The separators between the columns are tab characters, which a
/// [`TabWriter`] turns into aligned columns.
pub const DEFAULT_TABLE_FORMAT: &str = concat!(
    "This application is configured via the environment. The following environment\n",
    "variables can be used:\n",
    "\n",
    "KEY\tTYPE\tDEFAULT\tREQUIRED\tDESCRIPTION\n",
    "{{range .}}{{usage_key .}}\t{{usage_type .}}\t{{usage_default .}}\t",
    "{{usage_required .}}\t{{usage_description .}}\n",
    "{{end}}"
);

fn usage_key(v: &VarInfo) -> Result<String, String> {
    Ok(v.key.clone())
}

fn usage_description(v: &VarInfo) -> Result<String, String> {
    Ok(v.desc.to_owned())
}

fn usage_type(v: &VarInfo) -> Result<String, String> {
    Ok(v.type_desc.to_owned())
}

fn usage_default(v: &VarInfo) -> Result<String, String> {
    Ok(v.default.to_owned())
}

/// Renders the declared `required` text: `true` when it parses as true, the
/// declared text when it is empty, and an error when it is not a boolean.
fn usage_required(v: &VarInfo) -> Result<String, String> {
    if v.required.is_empty() {
        return Ok(String::new());
    }
    match strconv::parse_bool(v.required) {
        Ok(true) => Ok("true".to_owned()),
        Ok(false) => Ok(v.required.to_owned()),
        Err(e) => Err(e.to_string()),
    }
}

/// The five functions available to a usage template.
pub fn default_functions() -> HashMap<String, UsageFn> {
    let mut m: HashMap<String, UsageFn> = HashMap::new();
    m.insert("usage_key".to_owned(), usage_key);
    m.insert("usage_description".to_owned(), usage_description);
    m.insert("usage_type".to_owned(), usage_type);
    m.insert("usage_default".to_owned(), usage_default);
    m.insert("usage_required".to_owned(), usage_required);
    m
}

/// Writes usage information to standard output using the default header and
/// table format.
pub fn usage<T: EnvConfig + ?Sized>(prefix: &str, spec: &T) -> Result<(), Error> {
    let stdout = std::io::stdout();
    let mut tabs = TabWriter::new(stdout.lock(), 1, 4, ' ');
    let result = usagef(prefix, spec, &mut tabs, DEFAULT_TABLE_FORMAT);
    // Go flushes unconditionally and returns the earlier error.
    let flushed = TabWriter::flush(&mut tabs).map_err(|e| Error::Io(e.to_string()));
    result.and(flushed)
}

/// Writes usage information to `out` using `format`.
pub fn usagef<T: EnvConfig + ?Sized>(
    prefix: &str,
    spec: &T,
    out: &mut dyn Write,
    format: &str,
) -> Result<(), Error> {
    let tmpl = Template::parse("envconfig", format, default_functions())?;
    usaget(prefix, spec, out, &tmpl)
}

/// Writes usage information to `out` using an already-parsed `template`.
pub fn usaget<T: EnvConfig + ?Sized>(
    prefix: &str,
    spec: &T,
    out: &mut dyn Write,
    template: &Template,
) -> Result<(), Error> {
    let infos = gather_info(prefix, spec);
    let mut rendered = String::new();
    template.execute(&mut rendered, &infos)?;
    out.write_all(rendered.as_bytes())
        .map_err(|e| Error::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(required: &'static str) -> VarInfo {
        VarInfo {
            name: "F",
            alt: "",
            key: "K".to_owned(),
            type_desc: "String",
            default: "",
            required,
            desc: "",
        }
    }

    #[test]
    fn usage_required_renders_go_values() {
        assert_eq!(usage_required(&info("")).unwrap(), "");
        assert_eq!(usage_required(&info("true")).unwrap(), "true");
        assert_eq!(usage_required(&info("True")).unwrap(), "true");
        assert_eq!(usage_required(&info("1")).unwrap(), "true");
        assert_eq!(usage_required(&info("false")).unwrap(), "false");
        assert!(usage_required(&info("banana")).is_err());
    }

    #[test]
    fn table_format_uses_tab_separators() {
        assert!(DEFAULT_TABLE_FORMAT.contains("KEY\tTYPE\tDEFAULT\tREQUIRED\tDESCRIPTION\n"));
        assert!(DEFAULT_TABLE_FORMAT.ends_with("{{end}}"));
    }

    #[test]
    fn list_format_matches_the_original_constant() {
        assert!(DEFAULT_LIST_FORMAT.starts_with(
            "This application is configured via the environment. The following environment\n"
        ));
        assert!(DEFAULT_LIST_FORMAT.contains("  [description] {{usage_description .}}\n"));
        assert!(DEFAULT_LIST_FORMAT.ends_with("{{usage_required .}}{{end}}\n"));
    }
    // The stdout path — `usage` itself — is exercised by `test_usage_default`
    // in `tests/usage_test.rs`, which runs it in a child process and compares
    // the captured bytes against `testdata/default_table.txt`. It cannot be
    // exercised from a unit test here: `usage` writes to the process's real
    // stdout, and libtest's capture only intercepts the `print!` macros, so the
    // table lands in the middle of libtest's own result line and corrupts the
    // report every consumer of `cargo test` output parses.

    #[test]
    fn usaget_renders_a_prepared_template() {
        #[derive(Default, crate::EnvConfig)]
        struct Spec {
            debug: bool,
        }
        let tmpl = crate::gostd::template::Template::parse(
            "envconfig",
            "{{range .}}{{usage_key .}}|{{usage_type .}}{{end}}",
            default_functions(),
        )
        .unwrap();
        let mut buf: Vec<u8> = Vec::new();
        usaget("env_config", &Spec::default(), &mut buf, &tmpl).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "ENV_CONFIG_DEBUG|True or False"
        );
    }

    #[test]
    fn an_unparseable_required_value_fails_the_render() {
        #[derive(Default, crate::EnvConfig)]
        struct Spec {
            #[envconfig(required = "banana")]
            debug: bool,
        }
        let mut buf: Vec<u8> = Vec::new();
        let err = usagef(
            "env_config",
            &Spec::default(),
            &mut buf,
            DEFAULT_LIST_FORMAT,
        )
        .expect_err("expected the required value to fail to parse");
        assert!(err.to_string().contains("ParseBool"), "{err}");
    }

    #[test]
    fn a_bad_format_fails_before_rendering() {
        #[derive(Default, crate::EnvConfig)]
        struct Spec {
            debug: bool,
        }
        let mut buf: Vec<u8> = Vec::new();
        assert!(usagef("env_config", &Spec::default(), &mut buf, "{{nope .}}").is_err());
    }
}
