# Taking Data From The Environment

Environment variables are specified as a data source using the `--env` / `-e` command-line argument or the `env` prefix in the `CONTEMPLATE_DATASOURCES` environment variable:

=== "Command-Line"
    ```bash
    contemplate --env
    ```
=== "Environment"
    ```bash
    env CONTEMPLATE_DATASOURCES="env" contemplate
    ```

Since the environment variables of the contemplate process cannot be changed during runtime, environment data sources do not support live-reloading.

Environment variable names are [normalized](overview.md#data-normalization).

## Prefix Filtering

An optional prefix can be specified on the command-line or in the `CONTEMPLATE_DATASOURCES` environment variable.
In this case, only environment values with that prefix are considered by contemplate, and the prefix is dropped from the name.

In the following example, only the environment variables `FOO_BAR` and `FOO_BAZ` will be considered, while `QUX` will be ignored:

=== "Command-Line"
    ```bash
    env FOO_BAR=1 FOO_BAZ=2, QUX=3 contemplate --env=FOO
    ```
=== "Environment"
    ```bash
    env CONTEMPLATE_DATASOURCES="env:FOO" FOO_BAR=1 FOO_BAZ=2, QUX=3 contemplate
    ```

After processing, the context will contain the following values, since the prefix is dropped:
```json
{
    "bar": 1,
    "baz": 2
}
```

## Structured Values

Environment variables can only store strings.
However, Contemplate will parse the values of environment variables, and construct more complex values where appropriate.

* **Booleans**: `true` and `false` (case sensitive!) will be converted to their corresponding boolean values.
* **Numbers**: Any value that contains only digits, and optionally a single decimal separator (`.`) and/or a `-` sign prefix, will be parsed as a number, e.g.: `NUMBER=1.1`.
* **Lists**: Comma-separated values, delimited by `[` and `]`, will be parsed as a list, e.g.: `VALUES=[1.0, 3, true]`.
* **Dictionaries**: Comma-separated key-value pairs, separated by `=` and delimited by `{` and `}` will be parsed as a dictionary, e.g.: `COLOR={red=20, green=30, blue=10}`.

In ambiguous cases, interpretation as a string can be enforced using double quotes (`"`).
For more information, including details on escaping, please refer to the [figment documentation].

[figment documentation]: https://docs.rs/figment/latest/figment/providers/struct.Env.html
