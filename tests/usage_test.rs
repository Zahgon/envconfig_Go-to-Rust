//! Port of `usage_test.go`.

mod common;

use std::io::Write;
use std::process::Command;

use common::*;
use envconfig::gostd::tabwriter::TabWriter;
use envconfig::{DEFAULT_LIST_FORMAT, DEFAULT_TABLE_FORMAT};

/// Go's `TestMain` reads the four golden files before any test runs. Each is
/// loaded here on demand instead, which has the same effect.
fn golden(name: &str) -> String {
    let path = format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn test_usage_table_result() -> String {
    golden("default_table.txt")
}

fn test_usage_list_result() -> String {
    golden("default_list.txt")
}

fn test_usage_custom_result() -> String {
    golden("custom.txt")
}

fn test_usage_bad_format_result() -> String {
    golden("fault.txt")
}

/// Go's `compareUsage`: every space in the produced output is replaced by `.`
/// before comparing, and a mismatch reports the lengths, the first differing
/// index, and both strings in full.
fn compare_usage(want: &str, got: &str) {
    let got = got.replace(' ', ".");
    if want == got {
        return;
    }
    let mut report = String::new();
    if want.len() != got.len() {
        report.push_str(&format!(
            "expected result length of {}, found {}\n",
            want.len(),
            got.len()
        ));
    }
    let shortest = want.len().min(got.len());
    let (wb, gb) = (want.as_bytes(), got.as_bytes());
    for i in 0..shortest {
        if wb[i] != gb[i] {
            report.push_str(&format!(
                "difference at index {i}, expected '{}' ({}), found '{}' ({})\n",
                wb[i] as char, wb[i], gb[i] as char, gb[i]
            ));
            break;
        }
    }
    panic!("{report}Complete Expected:\n'{want}'\nComplete Found:\n'{got}'\n");
}

/// Renders the default table exactly as `usage` does, into a buffer.
fn render_default_table(spec: &Specification) -> String {
    let mut out: Vec<u8> = Vec::new();
    {
        let mut tabs = TabWriter::new(&mut out, 1, 4, ' ');
        envconfig::usagef("env_config", spec, &mut tabs, DEFAULT_TABLE_FORMAT).unwrap();
        TabWriter::flush(&mut tabs).unwrap();
    }
    String::from_utf8(out).unwrap()
}

const BEGIN: &str = "<<<ENVCONFIG-USAGE-BEGIN>>>";
const END: &str = "<<<ENVCONFIG-USAGE-END>>>";

/// Go redirects `os.Stdout` to a pipe, which is possible because `os.Stdout`
/// is a reassignable package variable. Rust's `stdout` is not, so the default
/// entry point is exercised in a child process and its real standard output is
/// captured, with sentinels marking the bytes `usage` itself wrote.
#[test]
fn test_usage_default() {
    if std::env::var_os("ENVCONFIG_USAGE_CHILD").is_some() {
        let s = Specification::default();
        clearenv();
        print!("{BEGIN}");
        std::io::stdout().flush().unwrap();
        envconfig::usage("env_config", &s).unwrap();
        print!("{END}");
        std::io::stdout().flush().unwrap();
        return;
    }

    let _g = guard();
    let exe = std::env::current_exe().expect("current exe");
    let output = Command::new(exe)
        .args(["--exact", "test_usage_default", "--nocapture"])
        .env("ENVCONFIG_USAGE_CHILD", "1")
        .output()
        .expect("spawn child");
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let start = stdout.find(BEGIN).expect("begin sentinel") + BEGIN.len();
    let end = stdout.find(END).expect("end sentinel");
    let out = &stdout[start..end];

    compare_usage(&test_usage_table_result(), out);
}

#[test]
fn test_usage_table() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    compare_usage(&test_usage_table_result(), &render_default_table(&s));
}

#[test]
fn test_usage_list() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    let mut buf: Vec<u8> = Vec::new();
    envconfig::usagef("env_config", &s, &mut buf, DEFAULT_LIST_FORMAT).unwrap();
    compare_usage(&test_usage_list_result(), &String::from_utf8(buf).unwrap());
}

#[test]
fn test_usage_custom_format() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    let mut buf: Vec<u8> = Vec::new();
    envconfig::usagef(
        "env_config",
        &s,
        &mut buf,
        "{{range .}}{{usage_key .}}={{usage_description .}}\n{{end}}",
    )
    .unwrap();
    compare_usage(
        &test_usage_custom_result(),
        &String::from_utf8(buf).unwrap(),
    );
}

#[test]
fn test_usage_unknown_key_format() {
    let _g = guard();
    let s = Specification::default();
    let unknown_error = "template: envconfig:1:2: executing \"envconfig\" at <.UnknownKey>";
    clearenv();
    let mut buf: Vec<u8> = Vec::new();
    let err = envconfig::usagef("env_config", &s, &mut buf, "{{.UnknownKey}}")
        .expect_err("expected 'unknown key' error, but got no error");
    assert!(
        err.to_string().contains(unknown_error),
        "expected '{unknown_error}', but got '{err}'"
    );
}

#[test]
fn test_usage_bad_format() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    // If you don't use two {{}} then you get a literal.
    let mut buf: Vec<u8> = Vec::new();
    envconfig::usagef("env_config", &s, &mut buf, "{{range .}}{.Key}\n{{end}}").unwrap();
    compare_usage(
        &test_usage_bad_format_result(),
        &String::from_utf8(buf).unwrap(),
    );
}
