//! HTTP calls to the daemon.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Blob, FormData, Headers, RequestInit, Response};

use crate::dom::{err_msg, win};
use crate::js::Js;

/// Send a request and return the response whatever its status. `body` is
/// sent as JSON.
pub async fn send(method: &str, path: &str, body: Option<JsValue>) -> Result<Response, String> {
    let init = RequestInit::new();
    init.set_method(method);
    if let Some(body) = body {
        let headers = Headers::new().map_err(|e| err_msg(&e))?;
        headers
            .set("Content-Type", "application/json")
            .map_err(|e| err_msg(&e))?;
        init.set_headers(&headers);
        let text = js_sys::JSON::stringify(&body).map_err(|e| err_msg(&e))?;
        init.set_body(&text);
    }
    fetch(path, &init).await
}

/// POST a multipart form and fail on a non-2xx status.
pub async fn post_form(path: &str, form: &FormData) -> Result<Response, String> {
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(form);
    check(fetch(path, &init).await?).await
}

async fn fetch(path: &str, init: &RequestInit) -> Result<Response, String> {
    let resp = JsFuture::from(win().fetch_with_str_and_init(path, init))
        .await
        .map_err(|e| err_msg(&e))?;
    resp.dyn_into()
        .map_err(|_| "fetch did not return a response".to_string())
}

/// Turn a non-2xx response into an error. The message is the `error` field
/// of a JSON body, else the body text, else the status code.
pub async fn check(resp: Response) -> Result<Response, String> {
    if resp.ok() {
        return Ok(resp);
    }
    let body = text(&resp).await.unwrap_or_default();
    let from_json = Js::parse(&body).ok().and_then(|j| j.get("error").str());
    Err(match from_json {
        Some(message) => message,
        None if body.trim().is_empty() => format!("HTTP {}", resp.status()),
        None => body,
    })
}

pub async fn text(resp: &Response) -> Result<String, String> {
    let promise = resp.text().map_err(|e| err_msg(&e))?;
    let value = JsFuture::from(promise).await.map_err(|e| err_msg(&e))?;
    Ok(value.as_string().unwrap_or_default())
}

pub async fn blob(resp: &Response) -> Result<Blob, String> {
    let promise = resp.blob().map_err(|e| err_msg(&e))?;
    let value = JsFuture::from(promise).await.map_err(|e| err_msg(&e))?;
    value
        .dyn_into()
        .map_err(|_| "response is not a blob".to_string())
}

/// Send a request, failing on a non-2xx status.
pub async fn call(method: &str, path: &str, body: Option<JsValue>) -> Result<Response, String> {
    check(send(method, path, body).await?).await
}

/// Send a request and parse the JSON response, failing on a non-2xx status.
pub async fn json(method: &str, path: &str, body: Option<JsValue>) -> Result<Js, String> {
    let resp = call(method, path, body).await?;
    let promise = resp.json().map_err(|e| err_msg(&e))?;
    JsFuture::from(promise)
        .await
        .map(Js)
        .map_err(|e| err_msg(&e))
}

pub async fn get(path: &str) -> Result<Js, String> {
    json("GET", path, None).await
}

pub async fn post(path: &str, body: JsValue) -> Result<Js, String> {
    json("POST", path, Some(body)).await
}

/// POST with no body, ignoring the response body.
pub async fn post_empty(path: &str) -> Result<(), String> {
    call("POST", path, None).await.map(drop)
}

/// POST a JSON body, ignoring the response body.
pub async fn post_only(path: &str, body: JsValue) -> Result<(), String> {
    call("POST", path, Some(body)).await.map(drop)
}

pub async fn delete(path: &str) -> Result<(), String> {
    call("DELETE", path, None).await.map(drop)
}

/// Percent-encode one path segment.
pub fn enc(segment: &str) -> String {
    js_sys::encode_uri_component(segment).into()
}
