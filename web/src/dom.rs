//! Small DOM helpers. `El` wraps an element that may be missing, so a lookup
//! that finds nothing turns every later call into a no-op instead of a panic.

use std::future::Future;

use js_sys::{Function, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    AddEventListenerOptions, CanvasRenderingContext2d, Document, Element, Event, EventTarget,
    HtmlCanvasElement, HtmlElement, HtmlInputElement, Node, Window,
};

pub fn win() -> Window {
    web_sys::window().expect("no window")
}

pub fn doc() -> Document {
    win().document().expect("no document")
}

/// Read a property of any JS object; `undefined` if it is missing.
pub fn prop(obj: &JsValue, key: &str) -> JsValue {
    Reflect::get(obj, &key.into()).unwrap_or(JsValue::UNDEFINED)
}

/// Call a zero-argument method by name.
pub fn call0(obj: &JsValue, method: &str) -> JsValue {
    match prop(obj, method).dyn_into::<Function>() {
        Ok(f) => f.call0(obj).unwrap_or(JsValue::UNDEFINED),
        Err(_) => JsValue::UNDEFINED,
    }
}

/// Call a one-argument method by name.
pub fn call1(obj: &JsValue, method: &str, arg: &JsValue) -> JsValue {
    match prop(obj, method).dyn_into::<Function>() {
        Ok(f) => f.call1(obj, arg).unwrap_or(JsValue::UNDEFINED),
        Err(_) => JsValue::UNDEFINED,
    }
}

/// The message of a thrown JS value.
pub fn err_msg(e: &JsValue) -> String {
    prop(e, "message")
        .as_string()
        .or_else(|| e.as_string())
        .unwrap_or_else(|| "unknown error".into())
}

pub fn spawn(f: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(f);
}

// ─── Events and timers ───────────────────────────────────────────────────────

/// Add a listener that lives for the rest of the page.
pub fn listen(target: &EventTarget, event: &str, f: impl FnMut(Event) + 'static) {
    let cb = Closure::<dyn FnMut(Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref());
    cb.forget();
}

/// Add a listener that is removed after it first fires.
pub fn listen_once(target: &EventTarget, event: &str, f: impl FnOnce(Event) + 'static) {
    let cb = Closure::once_into_js(f);
    let opts = AddEventListenerOptions::new();
    opts.set_once(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        event,
        cb.unchecked_ref(),
        &opts,
    );
}

pub fn doc_on(event: &str, f: impl FnMut(Event) + 'static) {
    listen(&doc(), event, f);
}

pub fn set_timeout(ms: i32, f: impl FnOnce() + 'static) -> i32 {
    let cb = Closure::once_into_js(f);
    win()
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), ms)
        .unwrap_or(0)
}

pub fn clear_timeout(id: i32) {
    win().clear_timeout_with_handle(id);
}

pub fn set_interval(ms: i32, f: impl FnMut() + 'static) -> i32 {
    let cb = Closure::<dyn FnMut()>::new(f);
    let id = win()
        .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), ms)
        .unwrap_or(0);
    cb.forget();
    id
}

pub fn clear_interval(id: i32) {
    win().clear_interval_with_handle(id);
}

/// Run an async task now and then every `ms` milliseconds.
pub fn poll<Fut: Future<Output = ()> + 'static>(ms: i32, f: fn() -> Fut) {
    spawn(f());
    set_interval(ms, move || spawn(f()));
}

pub fn next_frame(f: impl FnOnce() + 'static) {
    let cb = Closure::once_into_js(f);
    let _ = win().request_animation_frame(cb.unchecked_ref());
}

pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// The current time in the browser's locale, e.g. `14:03:27`.
pub fn time_str() -> String {
    locale_time(now_ms())
}

pub fn locale_time(ms: f64) -> String {
    let date = js_sys::Date::new(&JsValue::from(ms));
    call0(&date, "toLocaleTimeString")
        .as_string()
        .unwrap_or_default()
}

// ─── Browser services ────────────────────────────────────────────────────────

pub fn storage_get(key: &str) -> Option<String> {
    win().local_storage().ok()??.get_item(key).ok()?
}

pub fn storage_set(key: &str, value: &str) {
    if let Ok(Some(storage)) = win().local_storage() {
        let _ = storage.set_item(key, value);
    }
}

/// A list of strings stored as a JSON array; empty if missing or malformed.
pub fn storage_list(key: &str) -> Vec<String> {
    storage_get(key)
        .and_then(|raw| crate::js::Js::parse(&raw).ok())
        .map(|v| v.arr().iter().filter_map(super::js::Js::str).collect())
        .unwrap_or_default()
}

pub fn storage_set_list(key: &str, list: &[String]) {
    let json = crate::js::Js(crate::js::arr(
        list.iter().map(|s| JsValue::from(s.as_str())),
    ));
    storage_set(key, &json.to_json(false));
}

pub fn confirm(msg: &str) -> bool {
    win().confirm_with_message(msg).unwrap_or(false)
}

pub fn prompt(msg: &str) -> Option<String> {
    win().prompt_with_message(msg).ok().flatten()
}

pub fn alert(msg: &str) {
    let _ = win().alert_with_message(msg);
}

/// Whether keyboard focus is in a form field, where shortcuts must not fire.
pub fn typing() -> bool {
    matches!(
        doc().active_element().map(|e| e.tag_name()).as_deref(),
        Some("INPUT" | "TEXTAREA" | "SELECT")
    )
}

pub fn key(e: &Event) -> String {
    prop(e, "key").as_string().unwrap_or_default()
}

pub fn flag(e: &Event, name: &str) -> bool {
    prop(e, name).is_truthy()
}

/// The element an event was dispatched to.
pub fn target(e: &Event) -> El {
    El(e.target().and_then(|t| t.dyn_into().ok()))
}

// ─── Elements ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct El(pub Option<Element>);

impl El {
    pub fn id(id: &str) -> El {
        El(doc().get_element_by_id(id))
    }

    /// The first element matching a selector.
    pub fn q(selector: &str) -> El {
        El(doc().query_selector(selector).ok().flatten())
    }

    pub fn all(selector: &str) -> Vec<El> {
        collect(doc().query_selector_all(selector).ok())
    }

    pub fn new(tag: &str) -> El {
        El(doc().create_element(tag).ok())
    }

    /// A new element with a class and inner HTML.
    pub fn build(tag: &str, class: &str, html: &str) -> El {
        let el = El::new(tag);
        if !class.is_empty() {
            el.set_class(class);
        }
        el.set_html(html);
        el
    }

    pub fn body() -> El {
        El(doc().body().map(Into::into))
    }

    pub fn ok(&self) -> bool {
        self.0.is_some()
    }

    pub fn find(&self, selector: &str) -> El {
        El(self
            .0
            .as_ref()
            .and_then(|e| e.query_selector(selector).ok().flatten()))
    }

    pub fn find_all(&self, selector: &str) -> Vec<El> {
        collect(
            self.0
                .as_ref()
                .and_then(|e| e.query_selector_all(selector).ok()),
        )
    }

    pub fn closest(&self, selector: &str) -> El {
        El(self
            .0
            .as_ref()
            .and_then(|e| e.closest(selector).ok().flatten()))
    }

    pub fn parent(&self) -> El {
        El(self.0.as_ref().and_then(|e| e.parent_element()))
    }

    pub fn child_count(&self) -> u32 {
        self.0
            .as_ref()
            .map_or(0, web_sys::Element::child_element_count)
    }

    pub fn same(&self, other: &El) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    pub fn contains(&self, other: &El) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => a.contains(Some(b)),
            _ => false,
        }
    }

    // ── Content ──

    pub fn text(&self) -> String {
        self.0
            .as_ref()
            .and_then(|e| e.text_content())
            .unwrap_or_default()
    }

    pub fn set_text(&self, text: &str) -> &Self {
        if let Some(e) = &self.0 {
            e.set_text_content(Some(text));
        }
        self
    }

    pub fn set_html(&self, html: &str) -> &Self {
        if let Some(e) = &self.0 {
            e.set_inner_html(html);
        }
        self
    }

    /// The text of the first child node (the emoji of a mode button).
    pub fn first_text(&self) -> String {
        self.0
            .as_ref()
            .and_then(|e| e.first_child())
            .and_then(|n| n.text_content())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    // ── Properties and attributes ──

    pub fn get(&self, name: &str) -> JsValue {
        self.0
            .as_ref()
            .map_or(JsValue::UNDEFINED, |e| prop(e, name))
    }

    pub fn set<V: Into<JsValue>>(&self, name: &str, value: V) -> &Self {
        if let Some(e) = &self.0 {
            let _ = Reflect::set(e, &name.into(), &value.into());
        }
        self
    }

    /// The `value` of an input, select or textarea.
    pub fn val(&self) -> String {
        self.get("value").as_string().unwrap_or_default()
    }

    /// The `value`, trimmed.
    pub fn trimmed(&self) -> String {
        self.val().trim().to_string()
    }

    pub fn set_val(&self, value: &str) -> &Self {
        self.set("value", value)
    }

    pub fn checked(&self) -> bool {
        self.get("checked").is_truthy()
    }

    pub fn set_disabled(&self, disabled: bool) -> &Self {
        self.set("disabled", disabled)
    }

    pub fn attr(&self, name: &str) -> String {
        self.0
            .as_ref()
            .and_then(|e| e.get_attribute(name))
            .unwrap_or_default()
    }

    pub fn set_attr(&self, name: &str, value: &str) -> &Self {
        if let Some(e) = &self.0 {
            let _ = e.set_attribute(name, value);
        }
        self
    }

    /// A `data-*` attribute, or `""`.
    pub fn data(&self, name: &str) -> String {
        self.attr(&format!("data-{name}"))
    }

    pub fn set_data(&self, name: &str, value: &str) -> &Self {
        self.set_attr(&format!("data-{name}"), value)
    }

    pub fn title(&self) -> String {
        self.attr("title")
    }

    pub fn set_title(&self, title: &str) -> &Self {
        self.set_attr("title", title)
    }

    pub fn set_id(&self, id: &str) -> &Self {
        self.set_attr("id", id)
    }

    // ── Classes and style ──

    pub fn set_class(&self, class: &str) -> &Self {
        if let Some(e) = &self.0 {
            e.set_class_name(class);
        }
        self
    }

    pub fn class(&self, name: &str, on: bool) -> &Self {
        if let Some(e) = &self.0 {
            let _ = e.class_list().toggle_with_force(name, on);
        }
        self
    }

    pub fn has_class(&self, name: &str) -> bool {
        self.0
            .as_ref()
            .is_some_and(|e| e.class_list().contains(name))
    }

    pub fn css(&self, property: &str, value: &str) -> &Self {
        if let Some(e) = self.0.as_ref().and_then(|e| e.dyn_ref::<HtmlElement>()) {
            let _ = e.style().set_property(property, value);
        }
        self
    }

    pub fn css_text(&self, css: &str) -> &Self {
        self.set_attr("style", css)
    }

    pub fn color(&self, color: &str) -> &Self {
        self.css("color", color)
    }

    /// Set `display`; `""` restores the stylesheet's value.
    pub fn display(&self, value: &str) -> &Self {
        self.css("display", value)
    }

    pub fn show(&self) -> &Self {
        self.display("")
    }

    pub fn hide(&self) -> &Self {
        self.display("none")
    }

    pub fn is_hidden(&self) -> bool {
        self.0
            .as_ref()
            .and_then(|e| e.dyn_ref::<HtmlElement>())
            .and_then(|e| e.style().get_property_value("display").ok())
            .is_some_and(|d| d == "none")
    }

    pub fn rect(&self) -> (f64, f64, f64, f64) {
        self.0.as_ref().map_or((0.0, 0.0, 0.0, 0.0), |e| {
            let r = e.get_bounding_client_rect();
            (r.left(), r.top(), r.width(), r.height())
        })
    }

    // ── Tree edits ──

    pub fn append(&self, child: &El) -> &Self {
        if let (Some(p), Some(c)) = (&self.0, &child.0) {
            let _ = p.append_child(c);
        }
        self
    }

    pub fn prepend(&self, child: &El) -> &Self {
        if let (Some(p), Some(c)) = (&self.0, &child.0) {
            let _ = p.insert_before(c, p.first_child().as_ref());
        }
        self
    }

    fn adjacent(&self, position: &str, other: &El) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => a.insert_adjacent_element(position, b).is_ok(),
            _ => false,
        }
    }

    /// Insert `other` right after this element. False if either is missing.
    pub fn insert_after(&self, other: &El) -> bool {
        self.adjacent("afterend", other)
    }

    /// Insert `other` right before this element. False if either is missing.
    pub fn insert_before(&self, other: &El) -> bool {
        self.adjacent("beforebegin", other)
    }

    pub fn remove(&self) {
        if let Some(e) = &self.0 {
            e.remove();
        }
    }

    pub fn clear(&self) -> &Self {
        self.set_html("")
    }

    // ── Behaviour ──

    pub fn on(&self, event: &str, f: impl FnMut(Event) + 'static) -> &Self {
        if let Some(e) = &self.0 {
            listen(e, event, f);
        }
        self
    }

    /// Run an async handler on each event.
    pub fn on_async<F, Fut>(&self, event: &str, f: F) -> &Self
    where
        F: Fn(Event) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.on(event, move |e| spawn(f(e)))
    }

    pub fn click(&self) {
        self.call("click");
    }

    pub fn focus(&self) {
        self.call("focus");
    }

    pub fn call(&self, method: &str) {
        if let Some(e) = &self.0 {
            call0(e, method);
        }
    }

    /// Scroll just far enough to bring the element into view.
    pub fn scroll_into_view(&self) {
        if let Some(e) = &self.0 {
            call1(e, "scrollIntoView", &crate::obj! { "block": "nearest" });
        }
    }

    /// Show a blob in an `<img>`, releasing the blob it showed before.
    pub fn set_blob_src(&self, blob: &web_sys::Blob) {
        let old = self.attr("src");
        if old.starts_with("blob:") {
            let _ = web_sys::Url::revoke_object_url(&old);
        }
        if let Ok(url) = web_sys::Url::create_object_url_with_blob(blob) {
            self.set_attr("src", &url);
        }
    }

    pub fn first_file(&self) -> Option<web_sys::File> {
        self.0
            .as_ref()?
            .dyn_ref::<HtmlInputElement>()?
            .files()?
            .get(0)
    }

    /// The 2D context and pixel size of a canvas.
    pub fn canvas(&self) -> Option<(CanvasRenderingContext2d, f64, f64)> {
        let canvas = self.0.as_ref()?.dyn_ref::<HtmlCanvasElement>()?;
        let ctx = canvas.get_context("2d").ok()??.dyn_into().ok()?;
        Some((ctx, canvas.width() as f64, canvas.height() as f64))
    }
}

fn collect(list: Option<web_sys::NodeList>) -> Vec<El> {
    let Some(list) = list else { return Vec::new() };
    (0..list.length())
        .filter_map(|i| list.get(i))
        .filter_map(|n: Node| n.dyn_into::<Element>().ok())
        .map(|e| El(Some(e)))
        .collect()
}

/// A greyed-out single-line list item; `text` must be trusted HTML.
pub fn placeholder_item(text: &str) -> String {
    format!(r#"<li style="color:var(--overlay0);font-size:0.85rem">{text}</li>"#)
}

/// A new `<section class="card">` with the given id (if any) and content.
pub fn card(id: &str, html: &str) -> El {
    let section = El::build("section", "card", html);
    if !id.is_empty() {
        section.set_id(id);
    }
    section
}

/// Insert `section` after `anchor`, or at the end of `<main>` if the anchor
/// is missing.
pub fn place_after(anchor: &El, section: &El) {
    if !anchor.insert_after(section) {
        El::q("main").append(section);
    }
}
