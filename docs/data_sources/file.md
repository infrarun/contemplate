# File Data Sources

Files are among the simplest data sources. The following file types are supported:

* YAML (including [merge support](https://yaml.org/type/merge.html))
* TOML
* JSON

Files are identified by Contemplate using their file extension, and specified using the `--file` / `-f` command-line argument or the `file` prefix in the `CONTEMPLATE_DATASOURCES` environment variable

=== "Command-Line"
    ```bash
    contemplate --file data.yml --file data.toml --file data.json
    ```
=== "Environment"
    ```bash
    env CONTEMPLATE_DATASOURCES="file:data.yml,file:data.toml,file:data.json" contemplate
    ```

File data sources support live-reloading. When a file is changed, it will cause a re-render of the templates.
