//! WebAssembly implementation of the `HttpClient` bridge trait.
//!
//! This client forwards requests to the browser's `fetch` API and converts the
//! resulting `Response` objects back into the bridge-friendly `HttpResponse`
//! type. It intentionally keeps behaviour minimal (no automatic retries yet)
//! but honours per-request headers, bodies, and optional timeouts via
//! `AbortController`.

use async_trait::async_trait;
use bridge_traits::{
    error::{BridgeError, Result as BridgeResult},
    http::{HttpClient, HttpMethod, HttpRequest, HttpResponse},
    platform::DynAsyncRead,
};
use bytes::Bytes;
use futures::{
    future::{select, Either},
    io::Cursor,
    pin_mut, FutureExt,
};
use gloo_timers::future::TimeoutFuture;
use js_sys::{try_iter, Array, Uint8Array};
use std::{collections::HashMap, time::Duration};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, Request, RequestInit, RequestMode, Response, Window};

/// WebAssembly HTTP client backed by the browser's `fetch` API.
pub struct WasmHttpClient {
    window: Window,
}

impl WasmHttpClient {
    /// Create a new client bound to the current browser window.
    pub fn new() -> BridgeResult<Self> {
        let window =
            web_sys::window().ok_or_else(|| BridgeError::NotAvailable("window".to_string()))?;
        Ok(Self { window })
    }

    fn build_request(
        &self,
        request: &HttpRequest,
        signal: Option<&web_sys::AbortSignal>,
    ) -> BridgeResult<Request> {
        let init = RequestInit::new();
        init.set_method(method_to_str(request.method));
        init.set_mode(RequestMode::Cors);

        if let Some(signal) = signal {
            init.set_signal(Some(signal));
        }

        if let Some(body) = &request.body {
            let body_array = Uint8Array::from(body.as_ref());
            init.set_body(&JsValue::from(body_array));
        }

        let headers = web_sys::Headers::new().map_err(|err| js_error("create headers", err))?;
        for (key, value) in &request.headers {
            headers
                .set(key, value)
                .map_err(|err| js_error("set header", err))?;
        }
        init.set_headers(&headers);

        Request::new_with_str_and_init(&request.url, &init)
            .map_err(|err| js_error("build request", err))
    }

    async fn fetch_with_timeout(
        &self,
        req: &Request,
        controller: Option<AbortController>,
        timeout: Option<Duration>,
    ) -> BridgeResult<Response> {
        let fetch = JsFuture::from(self.window.fetch_with_request(req));

        let result = if let (Some(timeout), Some(controller)) = (timeout, controller) {
            let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
            let timeout_fut = TimeoutFuture::new(timeout_ms).map(|_| ());
            pin_mut!(timeout_fut);
            pin_mut!(fetch);

            match select(fetch, timeout_fut).await {
                Either::Left((response, _)) => response,
                Either::Right((_, pending_fetch)) => {
                    controller.abort();
                    // Ensure the fetch future is polled again to observe cancellation.
                    let _ = pending_fetch.await;
                    return Err(BridgeError::OperationFailed(format!(
                        "HTTP request timed out after {} ms",
                        timeout.as_millis()
                    )));
                }
            }
        } else {
            fetch.await
        };

        let js_value = result.map_err(|err| js_error("fetch", err))?;
        js_value
            .dyn_into::<Response>()
            .map_err(|_| BridgeError::OperationFailed("fetch returned non-Response".into()))
    }

    async fn read_body(response: &Response) -> BridgeResult<Bytes> {
        let promise = response
            .array_buffer()
            .map_err(|err| js_error("response.array_buffer", err))?;
        let buffer = JsFuture::from(promise)
            .await
            .map_err(|err| js_error("response buffer", err))?;
        let array = Uint8Array::new(&buffer);
        let mut bytes = vec![0u8; array.length() as usize];
        array.copy_to(&mut bytes);
        Ok(Bytes::from(bytes))
    }

    fn collect_headers(response: &Response) -> BridgeResult<HashMap<String, String>> {
        let headers = response.headers();
        let iterator = try_iter(&JsValue::from(headers.clone()))
            .map_err(|err| js_error("iterate headers", err))?
            .ok_or_else(|| BridgeError::OperationFailed("Headers iterator unavailable".into()))?;

        let mut map = HashMap::new();
        for entry in iterator {
            let entry = entry.map_err(|err| js_error("header iteration", err))?;
            let pair = Array::from(&entry);
            if pair.length() >= 2 {
                if let (Some(key), Some(value)) = (pair.get(0).as_string(), pair.get(1).as_string())
                {
                    map.insert(key, value);
                }
            }
        }

        Ok(map)
    }
}

#[async_trait(?Send)]
impl HttpClient for WasmHttpClient {
    async fn execute(&self, request: HttpRequest) -> BridgeResult<HttpResponse> {
        let controller = if request.timeout.is_some() {
            Some(AbortController::new().map_err(|err| js_error("create abort controller", err))?)
        } else {
            None
        };

        let signal = controller.as_ref().map(|c| c.signal());
        let req = self.build_request(&request, signal.as_ref())?;
        let response = self
            .fetch_with_timeout(&req, controller, request.timeout)
            .await?;
        let body = Self::read_body(&response).await?;
        let headers = Self::collect_headers(&response)?;

        Ok(HttpResponse {
            status: response.status(),
            headers,
            body,
        })
    }

    async fn download_stream(&self, url: String) -> BridgeResult<Box<DynAsyncRead>> {
        let response = self.execute(HttpRequest::new(HttpMethod::Get, url)).await?;
        let cursor = Cursor::new(response.body.to_vec());
        Ok(Box::new(cursor) as Box<DynAsyncRead>)
    }

    async fn is_connected(&self) -> bool {
        // Try a very small HEAD request against a known fast endpoint?
        // For now, rely on navigator.onLine to avoid extra requests.
        self.window.navigator().on_line()
    }
}

fn method_to_str(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
        HttpMethod::Head => "HEAD",
    }
}

fn js_error(context: &str, err: JsValue) -> BridgeError {
    let message = if err.is_string() {
        err.as_string().unwrap_or_default()
    } else if let Some(js_err) = err.dyn_ref::<js_sys::Error>() {
        js_err.message().into()
    } else {
        format!("{err:?}")
    };
    BridgeError::OperationFailed(format!("WasmHttpClient {context}: {message}"))
}
