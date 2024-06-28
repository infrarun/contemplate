use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    sync::Arc,
};

use tokio::sync::RwLock;

use arc_swap::ArcSwapOption;
use encoding_rs::{Encoding, UTF_8};
use mime::Mime;
use minijinja::{Error, ErrorKind, State, Value, value::Object};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Serialize;

use crate::functions::ContextWithRuntime;

#[derive(Default)]
pub struct RequestCache {
    inner: RwLock<HashSet<CachableRequest>>,
}

impl RequestCache {
    async fn insert(&self, request: CachableRequest) {
        self.inner.write().await.insert(request);
    }

    async fn get_cacheable_request(
        &self,
        method: reqwest::Method,
        url: reqwest::Url,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> CachableRequest {
        let new_request = CachableRequest::new(method, url, headers, body);

        if let Some(cached) = self.inner.read().await.get(&new_request) {
            log::debug!("Request found in cache");
            return cached.to_owned();
        }

        log::debug!("Request uncached");
        new_request
    }
}

impl Debug for RequestCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestCache")
            .field("inner", &self.inner)
            .finish()
    }
}

impl Object for RequestCache {}

#[derive(Debug, Serialize)]
struct Response {
    status: u16,
    headers: HashMap<String, String>,
    json: Option<serde_json::Value>,
    text: Option<String>,
}

#[derive(Debug)]
struct Cached {
    etag: Option<String>,
    response: Response,
}

impl From<Response> for Cached {
    fn from(response: Response) -> Self {
        let etag = response
            .headers
            .get(reqwest::header::ETAG.as_str())
            .cloned();
        Self { etag, response }
    }
}

struct CachableRequest {
    method: reqwest::Method,
    url: reqwest::Url,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    cached_response: ArcSwapOption<Cached>,
}

impl Clone for CachableRequest {
    fn clone(&self) -> Self {
        let cached_response = ArcSwapOption::new(self.cached_response.load_full().clone());
        Self {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            cached_response,
        }
    }
}

impl Hash for CachableRequest {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.url.hash(state);
        self.headers.hash(state);
        self.body.hash(state);
        // Do not hash cached response in.
    }
}

impl PartialEq for CachableRequest {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method
            && self.url == other.url
            && self.headers == other.headers
            && self.body == other.body
        // Do not compare cached response
    }
}

impl Eq for CachableRequest {}

impl Debug for CachableRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachableRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .field("cached_response", &self.cached_response)
            .finish()
    }
}

impl CachableRequest {
    pub fn new(
        method: reqwest::Method,
        url: reqwest::Url,
        headers: BTreeMap<String, String>,
        body: Vec<u8>,
    ) -> Self {
        Self {
            method,
            url,
            headers,
            body,
            cached_response: ArcSwapOption::empty(),
        }
    }

    pub async fn perform(&self) -> Result<Arc<Cached>, Error> {
        let headers = self
            .headers
            .iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect::<HashMap<_, _>>();
        log::trace!("headers: {headers:?}");

        let mut headers: HeaderMap = (&headers).try_into().map_err(|e| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("Could not serialize headers: {e}"),
            )
        })?;

        if let Some(etag) = self
            .cached_response
            .load()
            .as_ref()
            .and_then(|cached| cached.etag.clone())
        {
            let etag = HeaderValue::from_str(&etag).map_err(|e| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    format!("Could not create ETag header: {e}"),
                )
            })?;
            log::trace!("Setting If-None-Match to {etag:?}");
            headers.insert(reqwest::header::IF_NONE_MATCH, etag);
        }

        let client = reqwest::Client::new();
        let request = reqwest::Request::new(self.method.clone(), self.url.clone());

        let response = reqwest::RequestBuilder::from_parts(client, request)
            .headers(headers)
            .body(self.body.clone())
            .send()
            .await
            .map_err(|e| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    format!("Could not perform HTTP request: {e}"),
                )
            })?;
        log::trace!("Response: {response:?}");

        let status = response.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            log::debug!("Server returned 304 Not Modified");
            return self.cached_response.load_full().ok_or(Error::new(
                ErrorKind::InvalidOperation,
                "HTTP Server returned 304 Not Modified, but no response is cached.",
            ));
        }

        let content_type: Option<Mime> = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<Mime>().ok());

        let encoding_name = content_type
            .as_ref()
            .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
            .unwrap_or("utf-8");

        let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8);

        let headers = response
            .headers()
            .into_iter()
            .filter_map(|(k, v)| match (k.as_str(), v.to_str()) {
                (k, Ok(v)) => Some((k.to_string(), v.to_string())),
                _ => None,
            })
            .collect();

        let body = response.bytes().await.map_err(|e| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("Could not get body: {e}"),
            )
        })?;

        let text = match encoding.decode(&body) {
            (text, _, false) => Some(text.to_string()),
            (_, _, true) => None,
        };

        let cached: Arc<Cached> = Arc::new(
            Response {
                status: status.as_u16(),
                headers,
                json: serde_json::from_slice(body.as_ref()).ok(),
                text,
            }
            .into(),
        );

        self.cached_response.store(Some(cached.clone()));

        Ok(cached)
    }
}

pub fn http_request(
    state: &State,
    method: &str,
    url: &str,
    headers: Option<&Value>,
    body: Option<&Value>,
) -> Result<Value, Error> {
    let rt = crate::functions::get_runtime_handle(state);
    log::debug!("HTTP Request: Method={method:?} url={url:?}");

    let ctx = state
        .lookup("$context")
        .ok_or(Error::new(ErrorKind::InvalidOperation, "$context missing"))?;
    let ctx = ctx
        .downcast_object_ref::<ContextWithRuntime>()
        .ok_or(Error::new(
            ErrorKind::InvalidOperation,
            "Context has wrong type",
        ))?;

    let method = method
        .to_uppercase()
        .parse()
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("Invalid method: {e}")))?;

    let url = url
        .parse()
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("Invalid URL: {e}")))?;

    let headers = headers
        .and_then(|headers| {
            headers
                .as_object()
                .and_then(|obj| obj.try_iter_pairs())
                .map(|i| {
                    i.filter_map(|(k, v)| match (k.as_str(), v.as_str()) {
                        (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                        _ => None,
                    })
                    .collect()
                })
        })
        .unwrap_or_default();

    let body = match body {
        Some(body) => body
            .as_bytes()
            .ok_or(Error::new(ErrorKind::InvalidOperation, "Wrong body type!"))?
            .to_owned(),
        None => vec![],
    };

    rt.block_on(async {
        if !ctx.has::<RequestCache>().await {
            ctx.put(RequestCache::default()).await;
        }

        let cache = ctx
            .get_ref::<RequestCache>()
            .await
            .expect("Cache is missing.");

        let request = cache
            .get_cacheable_request(method, url, headers, body)
            .await;

        std::mem::drop(cache);

        let cached = request.perform().await?;

        let cache = ctx
            .get_mut::<RequestCache>()
            .await
            .expect("Cache is missing.");
        cache.insert(request).await;
        std::mem::drop(cache);

        Ok(Value::from_serialize(&cached.response))
    })
}
