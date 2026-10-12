//! A thin wrapper over `JsValue` for reading JSON responses, plus `obj!` for
//! building request bodies.

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::JsValue;

/// A JSON value from the API. Missing fields read as `undefined`, so lookups
/// chain without checks, as they do in JavaScript.
#[derive(Clone)]
pub struct Js(pub JsValue);

impl Js {
    pub fn parse(text: &str) -> Result<Js, String> {
        js_sys::JSON::parse(text)
            .map(Js)
            .map_err(|e| crate::dom::err_msg(&e))
    }

    pub fn get(&self, key: &str) -> Js {
        if self.0.is_object() {
            Js(Reflect::get(&self.0, &key.into()).unwrap_or(JsValue::UNDEFINED))
        } else {
            Js(JsValue::UNDEFINED)
        }
    }

    pub fn at(&self, idx: u32) -> Js {
        Js(Reflect::get_u32(&self.0, idx).unwrap_or(JsValue::UNDEFINED))
    }

    pub fn is_nullish(&self) -> bool {
        self.0.is_null() || self.0.is_undefined()
    }

    pub fn truthy(&self) -> bool {
        self.0.is_truthy()
    }

    pub fn str(&self) -> Option<String> {
        self.0.as_string()
    }

    /// The string value, or `""` for anything that is not a string.
    pub fn string(&self) -> String {
        self.str().unwrap_or_default()
    }

    pub fn num(&self) -> Option<f64> {
        self.0.as_f64()
    }

    pub fn num_or(&self, default: f64) -> f64 {
        self.num().unwrap_or(default)
    }

    /// The elements if this is an array, otherwise nothing.
    pub fn arr(&self) -> Vec<Js> {
        if Array::is_array(&self.0) {
            Array::from(&self.0).iter().map(Js).collect()
        } else {
            Vec::new()
        }
    }

    /// Text for showing a scalar to the user; empty for null and objects.
    pub fn show(&self) -> String {
        if let Some(s) = self.str() {
            s
        } else if let Some(n) = self.num() {
            n.to_string()
        } else if let Some(b) = self.0.as_bool() {
            b.to_string()
        } else {
            String::new()
        }
    }

    pub fn to_json(&self, pretty: bool) -> String {
        let indent = if pretty {
            JsValue::from(2)
        } else {
            JsValue::UNDEFINED
        };
        js_sys::JSON::stringify_with_replacer_and_space(&self.0, &JsValue::NULL, &indent)
            .ok()
            .and_then(|s| s.as_string())
            .unwrap_or_default()
    }
}

/// Set `key` on `o`, skipping `undefined` so `None` fields are left out of
/// the JSON body.
pub fn set_field<V: Into<JsValue>>(o: &Object, key: &str, value: V) {
    let value = value.into();
    if !value.is_undefined() {
        let _ = Reflect::set(o, &key.into(), &value);
    }
}

/// A JS array from anything convertible to `JsValue`.
pub fn arr<T: Into<JsValue>>(items: impl IntoIterator<Item = T>) -> JsValue {
    items.into_iter().map(Into::into).collect::<Array>().into()
}

/// A non-empty trimmed string, or `None` (which `obj!` leaves out).
pub fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Build a JSON object: `obj! { "mode": mode, "interval": 5 }`.
/// `Option` values that are `None` are omitted.
#[macro_export]
macro_rules! obj {
    ($($key:literal : $value:expr),* $(,)?) => {{
        let o = js_sys::Object::new();
        $( $crate::js::set_field(&o, $key, $value); )*
        wasm_bindgen::JsValue::from(o)
    }};
}
