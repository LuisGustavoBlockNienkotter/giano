//! Turso (libSQL) over the Hrana HTTP protocol (`POST /v2/pipeline`), synchronous.
//!
//! Each request carries a list of statements; the server answers with one result per
//! statement and a `baton` that continues the same *stream* (a server-side connection).
//! Outside a session every call opens a stream and closes it in the same round trip; inside
//! [`RemoteConnection::pin`] / [`RemoteConnection::unpin`] (a transaction, the migrations)
//! the baton is kept, so `BEGIN ... COMMIT` and `PRAGMA`s run on one connection.
//!
//! Every new stream starts with `PRAGMA foreign_keys = ON` in the same round trip (SQLite
//! keeps that setting per connection).
//!
//! Network and access failures become [`Error::Unavailable`] with a pt-BR message, so the UI
//! shows them like any refused operation.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value as Json};

use super::Value;
use crate::error::{Error, Result};

pub struct StmtResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected_row_count: u64,
    pub last_insert_rowid: Option<i64>,
}

pub struct RemoteConnection {
    agent: ureq::Agent,
    url: String,
    token: String,
    baton: RefCell<Option<String>>,
    /// The server may ask to continue a stream on another URL.
    stream_url: RefCell<Option<String>>,
    pinned: Cell<u32>,
}

/// `libsql://db-org.turso.io` → `https://db-org.turso.io`.
pub fn http_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    let lower = url.to_lowercase();
    for (scheme, http) in [("libsql://", "https://"), ("wss://", "https://"), ("ws://", "http://")] {
        if lower.starts_with(scheme) {
            return format!("{http}{}", &url[scheme.len()..]);
        }
    }
    url.to_string()
}

impl RemoteConnection {
    pub fn new(url: &str, token: &str) -> Result<RemoteConnection> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build();
        Ok(RemoteConnection {
            agent,
            url: http_url(url),
            token: token.trim().to_string(),
            baton: RefCell::new(None),
            stream_url: RefCell::new(None),
            pinned: Cell::new(0),
        })
    }

    pub fn pin(&self) {
        self.pinned.set(self.pinned.get() + 1);
    }

    /// End a session; the outermost one closes the stream.
    pub fn unpin(&self) -> Result<()> {
        let n = self.pinned.get().saturating_sub(1);
        self.pinned.set(n);
        if n == 0 && self.baton.borrow().is_some() {
            let result = self.pipeline(vec![json!({"type": "close"})], false);
            self.reset_stream();
            result?;
        }
        Ok(())
    }

    fn reset_stream(&self) {
        *self.baton.borrow_mut() = None;
        *self.stream_url.borrow_mut() = None;
    }

    pub fn execute(&self, sql: &str, params: &[Value]) -> Result<StmtResult> {
        let stmt = json!({
            "type": "execute",
            "stmt": {"sql": sql, "args": params.iter().map(encode).collect::<Vec<_>>(), "want_rows": true},
        });
        let mut results = self.request(stmt)?;
        decode_result(results.remove(0))
    }

    pub fn execute_script(&self, sql: &str) -> Result<()> {
        let mut results = self.request(json!({"type": "sequence", "sql": sql}))?;
        check(results.remove(0)).map(|_| ())
    }

    /// One request on the current stream (opening/closing it as needed).
    fn request(&self, req: Json) -> Result<Vec<Json>> {
        let new_stream = self.baton.borrow().is_none();
        let close = self.pinned.get() == 0;
        let mut reqs = Vec::new();
        if new_stream {
            reqs.push(json!({"type": "execute", "stmt": {"sql": "PRAGMA foreign_keys = ON"}}));
        }
        reqs.push(req);
        if close {
            reqs.push(json!({"type": "close"}));
        }
        let mut results = self.pipeline(reqs, close)?;
        if new_stream {
            results.remove(0);
        }
        results.truncate(1);
        Ok(results)
    }

    fn pipeline(&self, requests: Vec<Json>, closing: bool) -> Result<Vec<Json>> {
        let base = self.stream_url.borrow().clone().unwrap_or_else(|| self.url.clone());
        let body = json!({"baton": *self.baton.borrow(), "requests": requests});
        let response = self
            .agent
            .post(&format!("{}/v2/pipeline", base.trim_end_matches('/')))
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(body);
        let response = match response {
            Ok(r) => r,
            Err(e) => {
                self.reset_stream();
                return Err(translate_http(e));
            }
        };
        let resp: Json = response.into_json().map_err(|e| {
            self.reset_stream();
            unavailable(&format!("resposta inválida do Turso: {e}"))
        })?;
        if closing {
            self.reset_stream();
        } else {
            *self.baton.borrow_mut() = resp["baton"].as_str().map(str::to_string);
            if let Some(url) = resp["base_url"].as_str() {
                *self.stream_url.borrow_mut() = Some(url.to_string());
            }
        }
        match resp.get("results").and_then(Json::as_array) {
            Some(results) => Ok(results.clone()),
            None => Err(Error::Database(format!("resposta inesperada do Turso: {resp}"))),
        }
    }
}

fn unavailable(detail: &str) -> Error {
    Error::Unavailable(format!("Sem conexão com o Turso (verifique a internet). Detalhe: {detail}"))
}

fn translate_http(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Status(code, resp) => {
            let detail = format!("HTTP {code}: {}", resp.into_string().unwrap_or_default());
            if code == 401 || code == 403 {
                Error::Unavailable(format!(
                    "O Turso recusou o token (inválido ou expirado). Gere outro e configure de \
                     novo. Detalhe: {detail}"
                ))
            } else {
                Error::Unavailable(format!("O Turso recusou a conexão. Detalhe: {detail}"))
            }
        }
        ureq::Error::Transport(t) => unavailable(&t.to_string()),
    }
}

/// A pipeline result: `{"type": "ok", "response": ...}` or `{"type": "error", "error": ...}`.
fn check(result: Json) -> Result<Json> {
    match result["type"].as_str() {
        Some("ok") => Ok(result["response"].clone()),
        Some("error") => {
            let message = result["error"]["message"].as_str().unwrap_or("erro desconhecido");
            let code = result["error"]["code"].as_str().unwrap_or("");
            if message.to_lowercase().contains("constraint failed") || code.contains("CONSTRAINT") {
                Err(Error::Integrity(message.to_string()))
            } else {
                Err(Error::Database(message.to_string()))
            }
        }
        _ => Err(Error::Database(format!("resposta inesperada do Turso: {result}"))),
    }
}

fn decode_result(result: Json) -> Result<StmtResult> {
    let response = check(result)?;
    let r = &response["result"];
    let columns = r["cols"]
        .as_array()
        .map(|cols| cols.iter().map(|c| c["name"].as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let rows = r["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row.as_array().map(|vs| vs.iter().map(decode).collect()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    Ok(StmtResult {
        columns,
        rows,
        affected_row_count: r["affected_row_count"].as_u64().unwrap_or(0),
        last_insert_rowid: match &r["last_insert_rowid"] {
            Json::String(s) => s.parse().ok(),
            Json::Number(n) => n.as_i64(),
            _ => None,
        },
    })
}

pub fn encode(v: &Value) -> Json {
    match v {
        Value::Null => json!({"type": "null"}),
        Value::Integer(i) => json!({"type": "integer", "value": i.to_string()}),
        Value::Real(f) => json!({"type": "float", "value": f}),
        Value::Text(s) => json!({"type": "text", "value": s}),
        Value::Blob(b) => {
            json!({"type": "blob", "base64": base64::engine::general_purpose::STANDARD.encode(b)})
        }
    }
}

pub fn decode(v: &Json) -> Value {
    match v["type"].as_str() {
        Some("integer") => match &v["value"] {
            Json::String(s) => s.parse().map(Value::Integer).unwrap_or(Value::Null),
            Json::Number(n) => n.as_i64().map(Value::Integer).unwrap_or(Value::Null),
            _ => Value::Null,
        },
        Some("float") => v["value"].as_f64().map(Value::Real).unwrap_or(Value::Null),
        Some("text") => Value::Text(v["value"].as_str().unwrap_or("").to_string()),
        Some("blob") => {
            let b64 = v["base64"].as_str().unwrap_or("");
            let engine = base64::engine::general_purpose::GeneralPurpose::new(
                &base64::alphabet::STANDARD,
                base64::engine::GeneralPurposeConfig::new()
                    .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
            );
            engine.decode(b64).map(Value::Blob).unwrap_or(Value::Null)
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(http_url("libsql://db-org.turso.io"), "https://db-org.turso.io");
        assert_eq!(http_url("LIBSQL://x.io/"), "https://x.io");
        assert_eq!(http_url("https://x.io"), "https://x.io");
        assert_eq!(http_url("ws://localhost:8080"), "http://localhost:8080");
    }

    #[test]
    fn values_roundtrip() {
        for v in [
            Value::Null,
            Value::Integer(-42),
            Value::Integer(i64::MAX),
            Value::Real(1.5),
            Value::Text("ação → ✓".into()),
            Value::Blob(vec![0, 1, 2, 255]),
        ] {
            assert_eq!(decode(&encode(&v)), v);
        }
    }

    #[test]
    fn errors() {
        let err = check(json!({"type": "error", "error": {"message": "UNIQUE constraint failed: x"}}));
        assert!(matches!(err, Err(Error::Integrity(_))));
        let err = check(json!({"type": "error", "error": {"message": "no such table: x"}}));
        assert!(matches!(err, Err(Error::Database(_))));
    }
}
