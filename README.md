# Contemplate

[![Crates.io Version](https://img.shields.io/crates/v/contemplate)](https://crates.io/crates/contemplate)
[![Docs](https://img.shields.io/badge/docs-green)](https://infrarun.github.io/contemplate)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/infrarun/contemplate/build.yml?branch=main)
![Crates.io MSRV](https://img.shields.io/crates/msrv/contemplate)

Contemplate is a template rendering tool designed to render configuration templates.
While it takes inspiration from configuration management systems such as [ansible]'s [template][ansible-template] action, it is specifically not designed to be a full configuration management system.
Furthermore, it's designed to be run on the target system, and ships as a single static binary.

## Features

* Template language [based on jinja2][minijinja]
* Layered [data sources][datasource-overview]: [File][datasource-file], [Environment Variables][datasource-env], [Kubernetes ConfigMaps and Secrets][datasource-k8s]
* [Live re-rendering] of templates when data changes
* Notification of configuration changes for downstream daemons
* Builds as a static binary for use in distroless or scratch containers

## Usage in Containers

To add Contemplate to a container build, add the following build step to your `Dockerfile`:

```dockerfile
COPY --from=ghcr.io/infrarun/contemplate:latest /contemplate /contemplate
```

Please see the [documentation] for more information.

[ansible]: https://www.ansible.com/
[ansible-template]: https://docs.ansible.com/ansible/latest/collections/ansible/builtin/template_module.html
[minijinja]: https://github.com/mitsuhiko/minijinja
[documentation]: https://infrarun.github.io/contemplate
[datasource-overview]: https://infrarun.github.io/contemplate/data_sources/overview
[datasource-file]: https://infrarun.github.io/contemplate//data_sources/file
[datasource-env]: https://infrarun.github.io/contemplate/data_sources/environment
[datasource-k8s]: https://infrarun.github.io/contemplate/data_sources/kubernetes
[Live re-rendering]: https://infrarun.github.io/contemplate/advanced/integration/#live-reloading