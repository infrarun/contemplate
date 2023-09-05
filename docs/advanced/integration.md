# Integrating with Other Software

Apart from rendering configuration, Contemplate has features to ensure the consumers of the configuration are only started once rendering is complete, as well as notifying them of configuration changes.

## Live Reloading

When underlying data sources change, the configuration is re-rendered if the `--watch` command-line option is specified:

```bash
export CONTEMPLATE_DATASOURCES="..."
contemplate --watch
```

!!! note
    Watch mode is not supported when standard output is specified, since it does not support re-winding and re-rendering.
    Furthermore, at least one data source will need to support watch mode.

Software that does not take care of its own configuration file reloading will need to be notified of this change.
Contemplate supports this using either signals or a custom reload hook.

### Signaling

The `--on-reload-signal` argument takes two arguments: the signal to send, and the target process.
Signals can be specified both using their number or their name (i.e. the following would be equivalent: `SIGINT`, `INT`, `2`). The target process can be identified either using its name or its PID.  If a name is specified, all processes containing the specified name are signaled.

```
contemplate \
    --watch \
    --file file.yml \
    --template config.template nginx.conf \
    --on-reload-signal HUP nginx
```

### Reload Hook

If signaling is not sufficient to notify downstream software that the configuration has changed, a custom reload hook can be executed on reload.
This is specified using the `--on-reload-command`/`-r` or `--on-reload-execute`/`-R` command-line options.
The difference between these two options is that `--on-reload-command` requires the presence of a shell interpreter, and takes a single string argument that is executed as a shell command, while `--on-reload-exec` takes a path to an executable.
When executed, the `CONTEMPLATED_FILES` environment variable will be set to a comma-separated list of the changed files.

```bash
contemplate \
    --watch \
    --file data.yml \
    --template config.template /etc/postfix/main.cf \
    --on-reload-command "postfix reload"
```

!!! note
    When underlying data sources change rapidly, changes can be debounced by the on-reload hook by sleeping before notifying the target process.
    The on-reload hook will be terminated with the `SIGINT` signal before a new hook is executed.
    Implementors relying on this feature in combination with the `CONTEMPLATED_FILES` variable will need to account for previous values of `CONTEMPLATED_FILES` as well as inherent raciness.

## Waiting for Rendering to be Completed

Most software reads configuration files on startup.
Therefore, contemplate needs to be finished rendering configuration files by the time the software using it is started.
This can be achieved using the `--and-then-exec` / `-x` command-line option, which allows an executable to be run once rendering is completed. All following arguments will be passed on, up to a delimiting argument containing just a semi-colon (`;`).

```
contemplate \
  --template config.template app.cfg \
  -x /usr/bin/app -h 0.0.0.0 -p 8080 \;
  # any further args after \; go to contemplate
```

In cases where Contemplate will need to continue running, such as when doing live re-loading, it will fork itself and continue running in the child after the initial render.
The parent will execute the target program.

!!! note
    On linux, Contemplate will register a death signal before executing the target process, ensuring that the child is terminated when the target process is terminated. This is important for clean shutdown behavior in containerized environments. When Contemplate is used with `--and-then-exec` as the container's entrypoint, after rendering, the target process will be PID 1 in the container.
