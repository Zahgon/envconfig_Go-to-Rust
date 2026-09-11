# envconfig

```toml
[dependencies]
envconfig = "1.4"
```

## Documentation

See [docs.rs](https://docs.rs/envconfig)

## Usage

Set some environment variables:

```Bash
export MYAPP_DEBUG=false
export MYAPP_PORT=8080
export MYAPP_USER=Kelsey
export MYAPP_RATE="0.5"
export MYAPP_TIMEOUT="3m"
export MYAPP_USERS="rob,ken,robert"
export MYAPP_COLORCODES="red:1,green:2,blue:3"
```

Write some code:

```rust
use std::collections::HashMap;

use envconfig::{Duration, EnvConfig};

#[derive(Default, EnvConfig)]
struct Specification {
    debug: bool,
    port: i32,
    user: String,
    users: Vec<String>,
    rate: f32,
    timeout: Duration,
    color_codes: HashMap<String, i32>,
}

fn main() {
    let mut s = Specification::default();
    if let Err(err) = envconfig::process("myapp", &mut s) {
        eprintln!("{err}");
        std::process::exit(1);
    }

    println!("Debug: {}", s.debug);
    println!("Port: {}", s.port);
    println!("User: {}", s.user);
    println!("Rate: {:.6}", s.rate);
    println!("Timeout: {}", s.timeout);

    println!("Users:");
    for u in &s.users {
        println!("  {u}");
    }

    println!("Color codes:");
    for (k, v) in &s.color_codes {
        println!("  {k}: {v}");
    }
}
```

Results:

```Bash
Debug: false
Port: 8080
User: Kelsey
Rate: 0.500000
Timeout: 3m0s
Users:
  rob
  ken
  robert
Color codes:
  red: 1
  green: 2
  blue: 3
```

## Attribute Support

Envconfig supports the use of field attributes to specify alternate, default, and required
environment variables.

Go's implementation reads struct tags with `reflect` at run time. Rust has no
run-time reflection, so the same information is declared with `#[envconfig(…)]`
attributes and read by the derive macro at compile time.

For example, consider the following struct:

```rust
# use envconfig::EnvConfig;
#[derive(Default, EnvConfig)]
struct Specification {
    #[envconfig(name = "manual_override_1")]
    manual_override_1: String,
    #[envconfig(default = "foobar")]
    default_var: String,
    #[envconfig(required)]
    required_var: String,
    #[envconfig(ignored)]
    ignored_var: String,
    #[envconfig(split_words)]
    auto_split_var: String,
    #[envconfig(required, split_words)]
    required_and_auto_split_var: String,
}
```

Envconfig has automatic support for CamelCased field names when the
`split_words` attribute is supplied. Without this attribute, `auto_split_var`
above would look for an environment variable called `MYAPP_AUTOSPLITVAR`. With
the setting applied it will look for `MYAPP_AUTO_SPLIT_VAR`. Note that numbers
will get globbed into the previous word. If the setting does not do the
right thing, you may use a manual override.

Envconfig will process the value for `manual_override_1` by populating it with
the value for `MYAPP_MANUAL_OVERRIDE_1`. Without this attribute, it would have
instead looked up `MYAPP_MANUALOVERRIDE1`. With the `split_words` attribute
it would have looked up `MYAPP_MANUAL_OVERRIDE1`.

```Bash
export MYAPP_MANUAL_OVERRIDE_1="this will be the value"

# export MYAPP_MANUALOVERRIDE1="and this will not"
```

If envconfig can't find an environment variable value for `MYAPP_DEFAULTVAR`,
it will populate it with "foobar" as a default value.

If envconfig can't find an environment variable value for `MYAPP_REQUIREDVAR`,
it will return an error when asked to process the struct.  If
`MYAPP_REQUIREDVAR` is present but empty, envconfig will not return an error.

If envconfig can't find an environment variable in the form `PREFIX_MYVAR`, and there
is a `name` attribute defined, it will try to populate your variable with an environment
variable that directly matches the attribute in your struct definition:

```shell
export SERVICE_HOST=127.0.0.1
export MYAPP_DEBUG=true
```
```rust
# use envconfig::EnvConfig;
#[derive(Default, EnvConfig)]
struct Specification {
    #[envconfig(name = "SERVICE_HOST")]
    service_host: String,
    debug: bool,
}
```

Envconfig won't process a field with the `ignored` attribute set, even if a corresponding
environment variable is set.

Rust has no anonymous struct embedding, so the two ways Go expands a nested
struct are spelled explicitly: `#[envconfig(embedded)]` keeps the parent
prefix, and `#[envconfig(nested)]` uses the field's own key as the prefix.

Because Rust field names are `snake_case`, a field whose declared name cannot
be spelled as an identifier — an acronym such as `TTL` — can set it with
`#[envconfig(field_name = "TTL")]`. This name is what appears in error
messages and what the environment variable name is derived from.

## Supported Field Types

envconfig supports these field types:

  * `String`
  * `i8`, `i16`, `i32`, `i64`, `isize`
  * `u8`, `u16`, `u32`, `u64`, `usize`
  * `bool`
  * `f32`, `f64`
  * `Vec<T>` of any supported type (`Vec<u8>` takes the raw value, like Go's `[]byte`)
  * `HashMap<K, V>` and `BTreeMap<K, V>` (keys and values of any supported type)
  * `Option<T>` of any supported type
  * types implementing [`TextUnmarshaler`](https://docs.rs/envconfig/latest/envconfig/trait.TextUnmarshaler.html)
  * types implementing [`BinaryUnmarshaler`](https://docs.rs/envconfig/latest/envconfig/trait.BinaryUnmarshaler.html)
  * [`Duration`](https://docs.rs/envconfig/latest/envconfig/struct.Duration.html), which parses Go's duration syntax
  * [`Time`](https://docs.rs/envconfig/latest/envconfig/struct.Time.html), RFC 3339
  * [`Url`](https://docs.rs/envconfig/latest/envconfig/struct.Url.html)

Nested and embedded structs using these fields are also supported.

Integers are parsed with base detection: `0x10` is 16, `010` is 8, `0b101` is
5, and `_` may separate digits.

## Custom Decoders

Any field whose type implements `envconfig::Decoder` can control its own
deserialization:

```Bash
export DNS_SERVER=8.8.8.8
```

```rust
use std::net::IpAddr;

use envconfig::{BoxError, Decoder, EnvConfig};

#[derive(Default)]
struct IpDecoder(Option<IpAddr>);

impl Decoder for IpDecoder {
    fn decode(&mut self, value: &str) -> Result<(), BoxError> {
        self.0 = Some(value.parse()?);
        Ok(())
    }
}

#[derive(Default, EnvConfig)]
struct DnsConfig {
    #[envconfig(name = "DNS_SERVER")]
    address: IpDecoder,
}
```

Also, envconfig will use a `Setter` implementation — the counterpart of Go's
[`flag.Value`](https://godoc.org/flag#Value) interface — if one is present.

When a type implements more than one of the decoding traits, they are tried in
this order, matching the original: `Decoder`, `Setter`, `TextUnmarshaler`,
`BinaryUnmarshaler`, then the built-in rules.

## Usage output

`envconfig::usage` prints a table describing every variable the specification
reads. `usagef` and `usaget` render a caller-supplied format, using the
`usage_key`, `usage_description`, `usage_type`, `usage_default` and
`usage_required` functions.
