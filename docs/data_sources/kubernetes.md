# Kubernetes Data Sources

Contemplate can take values from Kubernetes ConfigMap and Secret resources.
These can be specified using the `--k8s-configmap` / `--k8s-secret` command-line argument or the `k8s-configmap`/`k8s-secret` prefix in the `CONTEMPLATE_DATASOURCES` environment variable:

=== "Command-Line"
    ```bash
    contemplate --k8s-configmap app-config --k8s-secret app-secret
    ```
=== "Environment"
    ```bash
    env CONTEMPLATE_DATASOURCES="k8s-configmap:app-config,k8s-secret:app-secret" contemplate
    ```

The Kubernetes context and namespace are taken from the user's `KUBECONFIG` environment variable or `~/.kube/config`, or, if that failed, the in-cluster configuration (`KUBERNETES_SERVICE_HOST`, `KUBERNETES_SERVICE_PORT` and service account token in `/var/run/secrets/kubernetes.io/serviceaccount/`).

The namespace can be overridden using the `--k8s-namespace` command-line argument.


Keys in Kubernetes ConfigMaps and Secrets are [normalized](overview.md#data-normalization).
