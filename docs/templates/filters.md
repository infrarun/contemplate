# Filters

The following filters were added to Contemplate. For a full list of filters, see the output of the `debug()` function:

```bash
echo "{{ debug().env.filters }}" | contemplate
```

## base64encode

This base64-encodes a string or a list of bytes, returning a base64 encoded string.

Example:
=== "Template"
    ```jinja2
    {{ "Hello" | base64encode }}
    {{ [0,0,0,0] | base64encode }}
    ```
=== "Rendered"
    ```base64
    SGVsbG8=
    AAAAAA==
    ```

## hexencode

This hex-encodes a string or a list of bytes, returning a base64 encoded string.

Example:
=== "Template"
    ```jinja2
    {{ "Hello" | hexencode }}
    {{ [0,0,0,0] | hexencode }}
    ```
=== "Rendered"
    ```base64
    48656c6c6f
    00000000
    ```

## from_json

Constructs an object from a JSON string.

Example:
=== "Template"
    ```jinja2
    The color is {{ ('{"color": "green"}' | from_json).color }}
    ```
=== "Rendered"
    ```
    The color is green
    ```

## from_toml

Constructs an object from a TOML string.

Example:
=== "Template"
    ```jinja2
    The color is {{ ('color="orange"' | from_toml).color }}
    ```
=== "Rendered"
    ```
    The color is orange
    ```

## from_yaml

Constructs an object from a YAML string.

Example:
=== "Template"
    ```jinja2
    The color is {{ ('color: purple' | from_yaml).color }}
    ```
=== "Rendered"
    ```
    The color is purple
    ```
