# Templates

Templates are content written in `minijinja`, a templating language [close to `jinja2`][minijinja-compat].

Contemplate provides additional [filters](filters.md) and [functions](functions.md) on top of the standard minijinja built-ins.

## Standard In-/Output
By default, Contemplate will read a template on standard input, and write to standard output:

```bash
$ export NAME="Alice"
$ echo 'Hello, I am {{ name }}!' | contemplate -q --env
Hello, I am Alice!
$
```

!!! note
    A log message will be printed when reading from standard input, prompting the user to enter a template. This is suppressed in the above example using `-q`.

The output can be directed to a file by specifying it using `--output`/`-o`:

```bash
$ export NAME="Bob"
$ echo 'Hello, I am {{ name }}!' | contemplate --env -q -o output.txt
$ cat output.txt
Hello, I am Bob!
$
```

An input file can be specified as a positional argument:

```bash
$ export NAME="Charlie"
$ echo 'Hello, I am {{ name }}!' > template.txt
$ contemplate template.txt --env
Hello, I am Charlie!
$
```

## In-Place Rendering

If the source and target file are identical, in-place rendering can be used. This is enabled using the `--in-place`/`-i` command-line parameter. When enabled, multiple templates can be specified as positional arguments, each of which will be **overwritten** with the rendered context:

```bash
contemplate -i template1 template2
```

A backup of the original template can be automatically made by specifying a backup extension. The following will back-up the templates as `template1.bak` and `template2.bak`:

```bash
contemplate --in-place=bak template1 template2
```

!!! note
    To prevent backups from being overwritten by repeat invocations of Contemplate, it will refuse to overwrite backups.

## Multiple templates

Multiple templates can be specified on the command line with the `--template` / `-t` argument. Each `--template` argument takes two parameters, the input and output parameter. The following example renders the template contained in `input1.txt` to `output1.txt`, and the template contained in `input2.txt` to `output2.txt`.

```bash
contemplate \
  --template input1.txt output1.txt \
  --template input2.txt output2.txt
```

!!! note
    The output specification is optional. If left unspecified, output is directed to standard output. To specify standard input or standard output explicitly, `-` can be passed as a name, e.g. `--template - -` will cause Contemplate to read a template from standard input and write to standard output.
    However, it is an error to specify standard input as multiple source or destination values.

[minijinja-compat]: https://github.com/mitsuhiko/minijinja/blob/main/COMPATIBILITY.md

## Additional templates

Additional templates can be loaded from the file system with the `--additional-templates` / `-a` argument. The specified directory will be added as a search path. Template files contained within can now be referenced by their relative path. They are loaded on demand the first time they are referenced in a template.

For example, if you had the following file structure:

```
extra_templates
├── content
│   └── news.txt
└── macros.j2
```

Then you could call `contemplate -a extra_templates` and refer to the files as `macros.j2` and `content/news.txt`.

For details on how to refer to additional templates, read the minijinja documentation about [`extends`](https://docs.rs/minijinja/latest/minijinja/syntax/index.html#-extends-), [`include`](https://docs.rs/minijinja/latest/minijinja/syntax/index.html#-include-) and [`import`](https://docs.rs/minijinja/latest/minijinja/syntax/index.html#-import-).
