# Data Sources

Data sources provide the context for the template engine, i.e. the values to be inserted into the template.
To allow a flexible but structured way for values to have defaults, be overridden, or customizable at runtime, contemplate can use multiple data sources at the same time.
Each data source can add new values or override previous ones -- the context used by the template engine is the *stacked view* of all specified data sources.
Contemplate makes use of a framework named [figment] for this purpose.

## Command-Line Arguments
Data sources can be specified using command-line arguments, and are evaluated in order, left to right, with later specifications overriding earlier ones.

For example, in the following invocation, values will be taken from the file `data.yml`, and environment variables can override these:
```bash
contemplate --file data.yml --env
```

## The `CONTEMPLATE_DATASOURCES` Environment Variable
Data sources can furthermore be specified using the `CONTEMPLATE_DATASOURCES` environment variable. This should contain a comma-separated list of data source specifications, each of which is of the form `<type>[:<argument>]`:

```bash
env CONTEMPLATE_DATASOURCES="file:data.yml,env" contemplate
```

Data sources specified in the `CONTEMPLATE_DATASOURCES` environment variable are evaluated in order left-to-right, just like command-line arguments. However, they can be overridden using command-line arguments.

In the following example, values specified in `defaults.yml` will be overridden by values in `overrides.yml`:

```bash
env CONTEMPLATE_DATASOURCES="file:defaults.yml" contemplate --file overrides.yml
```

# Data Normalization

Many data sources, e.g. files, support specifying values in a nested-tree format:
=== "data.yml"
    ```yaml
    name: Alice
    age: 23
    pet:
      animal: dog
      name: Bob
    ```
=== "data.json"
    ```json
    {
      "name": "Alice",
      "age": 23,
      "pet": {
        "animal": "dog",
        "name": "Bob"
      }
    }
    ```
=== "data.toml"
    ```toml
    name = "Alice"
    age = 23

    [pet]
    animal = "dog"
    name = "Bob"
    ```

Others, such as environment variables, only allow key-value pairs, and are conventionally specified in all-caps:
```sh
NAME="Alice"
AGE=23
PET_NAME="Bob"
PET_ANIMAL="dog"
```

To bridge this gap, Contemplate will parse the variable names and construct a nested dictionary using the following rules:

* Underscores (`_`) symbolize descent into a nested dictionary
* Variable names are converted to lower-case

[figment]: https://github.com/SergioBenitez/Figment
