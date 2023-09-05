# Contemplate

Contemplate is a template rendering tool designed to render configuration templates.
While it takes inspiration from configuration management systems such as [ansible]'s [template][ansible-template] action, it is specifically not designed to be a full configuration management system.
Furthermore, it's designed to be run on the target system, and ships as a single static binary.

## Features

* Template language [based on jinja2][minijinja]
* Layered [data sources](data_sources/overview.md): [File](data_sources/file.md), [Environment Variables](data_sources/environment.md), [Kubernetes ConfigMaps and Secrets](data_sources/kubernetes.md)
* Live re-rendering of templates when data changes
* Notification of configuration changes for downstream daemons
* Builds as a static binary for use in distroless or scratch containers

## Usage in Containers

To add Contemplate to a container build, add the following build step to your `Dockerfile`:

```dockerfile
COPY --from=ghcr.io/infrarun/contemplate:latest /contemplate /contemplate
```

[ansible]: https://www.ansible.com/
[ansible-template]: https://docs.ansible.com/ansible/latest/collections/ansible/builtin/template_module.html
[minijinja]: https://github.com/mitsuhiko/minijinja
