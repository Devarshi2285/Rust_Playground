use js_sys::Function;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};
use std::sync::{Arc, Mutex};

// ── Wire protocol ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct RegisterMessage {
    event: String,
    id: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct SignalMessage {
    event: String,
    id: String,
    data: String,
}

// ── Callback storage ──────────────────────────────────────────────────────────

type CbStore = Arc<Mutex<Option<Function>>>;

fn empty_cb() -> CbStore {
    Arc::new(Mutex::new(None))
}

// ── Public struct ─────────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct SignalingClient {
    ws: WebSocket,
    client_id: String,

    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error:   Closure<dyn FnMut(ErrorEvent)>,
    _on_open:    Closure<dyn FnMut(JsValue)>,
    _on_close:   Closure<dyn FnMut(JsValue)>,

    cb_offer:  CbStore,
    cb_answer: CbStore,
    cb_ice:    CbStore,
    cb_open:   CbStore,
}

#[wasm_bindgen]
impl SignalingClient {
    /// `url`  – WebSocket URL e.g. "ws://localhost:9001"
    /// `id`   – unique client identifier sent to the server on connect
    #[wasm_bindgen(constructor)]
    pub fn new(url: &str, id: &str) -> Result<SignalingClient, JsValue> {
        let cb_offer  = empty_cb();
        let cb_answer = empty_cb();
        let cb_ice    = empty_cb();
        let cb_open   = empty_cb();

        let ws = WebSocket::new(url)?;
        let client_id = id.to_owned();

        // ── onopen: send register message ─────────────────────────────────
        let reg_json = serde_json::to_string(&RegisterMessage {
            event: "register".into(),
            id: client_id.clone(),
        }).unwrap();

        let ws_open = ws.clone();
        let cb_open_clone = cb_open.clone();
        let on_open = Closure::wrap(Box::new(move |_: JsValue| {
            web_sys::console::log_1(&"[SignalingClient] connected — sending register".into());
            let _ = ws_open.send_with_str(&reg_json);
            // fire user callback if set
            if let Some(f) = cb_open_clone.lock().unwrap().as_ref() {
                let _ = f.call0(&JsValue::NULL);
            }
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        // ── onclose ───────────────────────────────────────────────────────
        let on_close = Closure::wrap(Box::new(move |_: JsValue| {
            web_sys::console::log_1(&"[SignalingClient] closed".into());
        }) as Box<dyn FnMut(JsValue)>);
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        // ── onerror ───────────────────────────────────────────────────────
        let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
            web_sys::console::error_1(
                &format!("[SignalingClient] error: {}", e.message()).into()
            );
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        // ── onmessage ─────────────────────────────────────────────────────
        let cb_offer_m  = cb_offer.clone();
        let cb_answer_m = cb_answer.clone();
        let cb_ice_m    = cb_ice.clone();

        let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(txt) = e.data().as_string() {
                match serde_json::from_str::<SignalMessage>(&txt) {
                    Ok(sig) => {
                        // callback receives (id, data) as two separate string args
                        let cb = match sig.event.as_str() {
                            "offer"         => cb_offer_m.lock().ok().and_then(|g| g.clone()),
                            "answer"        => cb_answer_m.lock().ok().and_then(|g| g.clone()),
                            "ice-candidate" => cb_ice_m.lock().ok().and_then(|g| g.clone()),
                            _ => None,
                        };
                        if let Some(f) = cb {
                            let _ = f.call2(
                                &JsValue::NULL,
                                &JsValue::from_str(&sig.id),
                                &JsValue::from_str(&sig.data),
                            );
                        }
                    }
                    Err(err) => {
                        web_sys::console::error_1(
                            &format!("[SignalingClient] parse error: {}", err).into()
                        );
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        Ok(SignalingClient {
            ws,
            client_id,
            _on_message: on_message,
            _on_error:   on_error,
            _on_open:    on_open,
            _on_close:   on_close,
            cb_offer,
            cb_answer,
            cb_ice,
            cb_open,
        })
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    pub fn id(&self) -> String {
        self.client_id.clone()
    }

    /// Returns the WebSocket readyState (0=CONNECTING 1=OPEN 2=CLOSING 3=CLOSED)
    pub fn ready_state(&self) -> u16 {
        self.ws.ready_state()
    }

    // ── Event listeners ───────────────────────────────────────────────────────
    // All callbacks receive (id: string, data: string)

    pub fn on_open(&mut self, callback: Function) {
        *self.cb_open.lock().unwrap() = Some(callback);
    }

    pub fn on_offer(&mut self, callback: Function) {
        *self.cb_offer.lock().unwrap() = Some(callback);
    }

    pub fn on_answer(&mut self, callback: Function) {
        *self.cb_answer.lock().unwrap() = Some(callback);
    }

    pub fn on_ice_candidate(&mut self, callback: Function) {
        *self.cb_ice.lock().unwrap() = Some(callback);
    }

    // ── Send methods ──────────────────────────────────────────────────────────

    pub fn send_offer(&self, data: String) -> Result<(), JsValue> {
        self.send_signal("offer", &data)
    }

    pub fn send_answer(&self, data: String) -> Result<(), JsValue> {
        self.send_signal("answer", &data)
    }

    pub fn send_ice_candidate(&self, data: String) -> Result<(), JsValue> {
        self.send_signal("ice-candidate", &data)
    }

    pub fn close(&self) -> Result<(), JsValue> {
        self.ws.close()
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn send_signal(&self, event: &str, data: &str) -> Result<(), JsValue> {
        if self.ws.ready_state() != 1 {
            return Err(JsValue::from_str(
                "WebSocket not open — wait for on_open callback before sending"
            ));
        }
        let msg = SignalMessage {
            event: event.to_owned(),
            id:    self.client_id.clone(),
            data:  data.to_owned(),
        };
        let json = serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.ws.send_with_str(&json)
    }
}