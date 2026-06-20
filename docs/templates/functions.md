# Functions

The following functions are available in Contemplate templates.

## http

Performs an HTTP request and returns the response. This allows templates to fetch data from external APIs or services at render time.

```jinja2
http(method, url, headers={}, body=none)
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `method` | string | HTTP method (e.g. `"GET"`, `"POST"`). Case-insensitive. |
| `url` | string | The URL to request. |
| `headers` | object | Optional map of request headers. |
| `body` | bytes | Optional request body. |

**Return value:**

The function returns an object with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `status` | integer | HTTP response status code (e.g. `200`, `404`). |
| `headers` | object | Map of response header names to values. |
| `json` | object or null | The parsed JSON response body, if the response was valid JSON. Otherwise `null`. |
| `text` | string or null | The response body decoded as text, respecting the `charset` from the `Content-Type` header. `null` if decoding failed. |

**Caching:**

Responses are cached within a single render pass. If the same request (same method, URL, headers, and body) is made more than once in a template, only one HTTP request is sent. Contemplate also supports [ETag]-based conditional requests: if the server returns an `ETag` header, subsequent renders will include an `If-None-Match` header, and a `304 Not Modified` response will reuse the previously cached response body.

**Examples:**

Fetch JSON from an API and use a field:
=== "Template"
    ```jinja2
    {% set resp = http("GET", "https://api.example.com/config") %}
    {% if resp.status == 200 %}
    timeout={{ resp.json.timeout }}
    {% endif %}
    ```

Fetch with custom headers:
=== "Template"
    ```jinja2
    {% set resp = http("GET", "https://api.example.com/secret",
                       {"Authorization": "Bearer " ~ token}) %}
    value={{ resp.json.value }}
    ```

!!! note
    HTTP requests are made synchronously during template rendering. Long-running or failing requests will delay or prevent rendering. Use `--poll` to periodically re-render templates that depend on HTTP data.

[ETag]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/ETag
