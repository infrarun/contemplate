# Debugging

## Increasing Verbosity

The verbosity of `contemplate` can be increased by specifying `-v` (debug output) or `-vv` (trace output). Changes to rendered templates can be written to stderr by specifying the `--diff` option, while the `--dry-run`/`-n` command-line option will suppress the rendered template to be written.

## Templates

A list of all available filters can be dumped using the `debug()` built-in:
```bash
echo "{{ debug() }}" | contemplate
```
