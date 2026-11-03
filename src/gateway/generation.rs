//! Asking the Gateway what one generation cost, by its id.
//!
//! A stream that named its generation and said nothing of its bill
//! ([`super::protocol::Bill::Deferred`]) is reconciled later, the way upstream
//! does it: one authenticated `GET {origin}/v1/generation?id=<id>`
//! (`fetchGatewayGenerationResult`, `vercel-labs/fx@c1db9191
//! src/gateway/client.zig:285-372`), asked only of a trusted origin
//! ([`super::is_trusted_generation_origin`]), and read into the same facts a
//! stream's own bill carries (`parseGenerationRecord`,
//! `src/gateway/generation_usage.zig:93-178`).
//!
//! What a status means is upstream's too (`classifyStatus`,
//! `generation_usage.zig:58-67`): a bill the Gateway has not written yet is
//! asked for again, one the credential may not read is left pending for a
//! credential that may, and anything else is a bill that will never be known.

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{Map, Value};

use super::protocol::Billing;
use super::{is_trusted_generation_origin, USER_AGENT};

/// The route, under the generation's origin (`client.zig:319-325`).
pub const GENERATION_PATH: &str = "/v1/generation";

/// How long one question may take, connection included
/// (`generation_lookup_timeout_ms`, `client.zig:227`).
const TIMEOUT: Duration = Duration::from_millis(30_000);

/// The most of an answer that is read; a longer one is not a generation
/// record (`generation_response_max_bytes`, `client.zig:226`).
const MAX_BODY_BYTES: usize = 128 * 1024;

/// The longest model name a record may carry (`max_model_bytes`,
/// `session_usage.zig:22`).
const MAX_MODEL_BYTES: usize = 1024;

/// What one question about a generation came back with
/// (`LookupOutcome`, `generation_usage.zig:14-56`).
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// The bill, with the Gateway's own time for it.
    Found(Billing),
    /// Not written yet, or not answered: ask again.
    Retry,
    /// The credential may not read it (401, 403): leave it pending for one
    /// that may, and stop asking with this one.
    Preserve,
    /// It will never be known: an origin not trusted with the question, an
    /// answer that is not a record of this generation, or a status that says
    /// so.
    Reject,
}

/// What a non-200 status means (`classifyStatus`, `generation_usage.zig:58-67`).
pub fn classify(status: u16) -> Lookup {
    match status {
        404 | 408 | 425 | 429 => Lookup::Retry,
        500..=u16::MAX => Lookup::Retry,
        401 | 403 => Lookup::Preserve,
        _ => Lookup::Reject,
    }
}

/// Asks `origin` what generation `id` cost, with the bearer `secret`.
///
/// **No redirect is followed**, as upstream's `redirect_behavior = .unhandled`
/// (`client.zig:353-357`): the origin was judged trusted and the key goes to
/// it alone -- and so to no proxy over cleartext (`super::bearer_rules`). A
/// transport failure, a timeout and an answer too long to be a record are
/// asked again, as upstream's lookup error is (`generation_usage.zig:28-39`).
pub async fn lookup(origin: &str, secret: &str, id: &str) -> Lookup {
    if !is_trusted_generation_origin(origin) || !super::sse::is_generation_id(id) {
        return Lookup::Reject;
    }
    let Ok(client) = super::bearer_rules(
        reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT),
        origin,
    )
    .build() else {
        return Lookup::Retry;
    };
    let Ok(mut authorization) = reqwest::header::HeaderValue::from_str(&format!("Bearer {secret}"))
    else {
        return Lookup::Reject;
    };
    // Keeps the credential out of reqwest's own diagnostics.
    authorization.set_sensitive(true);
    let url = format!("{origin}{GENERATION_PATH}?id={id}");
    let Ok(response) = client
        .get(url)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .send()
        .await
    else {
        return Lookup::Retry;
    };
    let status = response.status().as_u16();
    if status != 200 {
        return classify(status);
    }
    let Some(body) = read_whole(response).await else {
        return Lookup::Retry;
    };
    match parse(&body, id) {
        Some(billing) => Lookup::Found(billing),
        None => Lookup::Reject,
    }
}

/// The body, or `None` when it broke off or ran past [`MAX_BODY_BYTES`].
async fn read_whole(response: reqwest::Response) -> Option<Vec<u8>> {
    let mut collected = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if collected.len() + chunk.len() > MAX_BODY_BYTES {
            return None;
        }
        collected.extend_from_slice(&chunk);
    }
    Some(collected)
}

/// One `GET /v1/generation` answer as the bill of generation `expected`, or
/// `None` when it is not one (`parseGenerationRecord`,
/// `generation_usage.zig:93-178`).
///
/// `{"data":{…}}` with the generation's `id` -- which must be `expected` --
/// its `model`, its `created_at` in the Gateway's UTC form, its `total_cost`,
/// and its native token counts. The prompt count is the uncached input, so
/// the input a stream would have said is prompt, cache reads and cache writes
/// together; the output is completion and reasoning together.
pub fn parse(body: &[u8], expected: &str) -> Option<Billing> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let data = value.as_object()?.get("data")?.as_object()?;
    let id = data.get("id")?.as_str()?;
    if !super::sse::is_generation_id(id) || id != expected {
        return None;
    }
    let model = data.get("model")?.as_str()?;
    if model.is_empty()
        || model.len() > MAX_MODEL_BYTES
        || !model.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return None;
    }
    let at_ms = super::sse::parse_gateway_timestamp(data.get("created_at")?.as_str()?)?;
    let cost = non_negative_number(data.get("total_cost")?)?;
    let prompt = whole(data, "native_tokens_prompt")?;
    let completion = whole(data, "native_tokens_completion")?;
    let cache_read_tokens = whole(data, "native_tokens_cached")?;
    let cache_write_tokens = whole(data, "native_tokens_cache_creation")?;
    let reasoning_tokens = match data.get("native_tokens_reasoning") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_u64()?),
    };
    // Read for the shape upstream requires of a record, and counted nowhere:
    // `/usage` has no row for search calls.
    whole(data, "billable_web_search_calls")?;
    Some(Billing {
        at_ms,
        model: model.to_string(),
        generation_id: Some(id.to_string()),
        cost,
        input_tokens: prompt
            .checked_add(cache_read_tokens)?
            .checked_add(cache_write_tokens)?,
        output_tokens: completion.checked_add(reasoning_tokens.unwrap_or(0))?,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
    })
}

/// A count that must be there and must be a whole number
/// (`parseNonNegativeInteger`, `generation_usage.zig:215-226`).
fn whole(data: &Map<String, Value>, key: &str) -> Option<u64> {
    data.get(key)?.as_u64()
}

/// A cost: any JSON number, finite and not negative
/// (`parseNonNegativeNumber`, `generation_usage.zig:200-213`).
fn non_negative_number(value: &Value) -> Option<f64> {
    let number = value.as_f64()?;
    (number.is_finite() && number >= 0.0).then_some(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "gen_01JABCDEFGHJKMNPQRSTVWXYZ1";

    fn record(changes: Value) -> Vec<u8> {
        let mut data = json!({
            "id": ID,
            "model": "anthropic/claude-opus-5",
            "created_at": "2026-10-07T01:02:03.456Z",
            "total_cost": 0.0123,
            "native_tokens_prompt": 1000,
            "native_tokens_completion": 200,
            "native_tokens_cached": 300,
            "native_tokens_cache_creation": 40,
            "native_tokens_reasoning": 50,
            "billable_web_search_calls": 0
        });
        for (key, value) in changes.as_object().expect("an object") {
            if value.is_null() && key != "native_tokens_reasoning" {
                data.as_object_mut().unwrap().remove(key);
            } else {
                data[key] = value.clone();
            }
        }
        serde_json::to_vec(&json!({ "data": data })).unwrap()
    }

    #[test]
    fn a_record_is_read_into_the_bill_a_stream_would_have_said() {
        let billing = parse(&record(json!({})), ID).expect("a record");
        assert_eq!(billing.generation_id.as_deref(), Some(ID));
        assert_eq!(billing.model, "anthropic/claude-opus-5");
        assert_eq!(billing.cost, 0.0123);
        assert_eq!(
            billing.at_ms,
            super::super::sse::parse_gateway_timestamp("2026-10-07T01:02:03.456Z").unwrap()
        );
        // Prompt, cache reads and cache writes are the input; completion and
        // reasoning the output (`generation_usage.zig:140-160`).
        assert_eq!((billing.input_tokens, billing.output_tokens), (1340, 250));
        assert_eq!(
            (billing.cache_read_tokens, billing.cache_write_tokens),
            (300, 40)
        );
        assert_eq!(billing.reasoning_tokens, Some(50));
        // Reasoning not said is not nought, and adds nothing.
        let billing =
            parse(&record(json!({ "native_tokens_reasoning": null })), ID).expect("a record");
        assert_eq!(
            (billing.output_tokens, billing.reasoning_tokens),
            (200, None)
        );
        // A whole-number cost is a cost.
        assert_eq!(
            parse(&record(json!({ "total_cost": 2 })), ID).map(|b| b.cost),
            Some(2.0)
        );
    }

    #[test]
    fn an_answer_that_is_not_this_generations_record_is_not_its_bill() {
        let other = "gen_01JABCDEFGHJKMNPQRSTVWXYZ9";
        for changes in [
            json!({ "id": other }),
            json!({ "id": "gen_lowercase" }),
            json!({ "model": "" }),
            json!({ "model": "a b" }),
            json!({ "created_at": "2026-10-07 01:02:03" }),
            json!({ "total_cost": -1 }),
            json!({ "total_cost": "0.1" }),
            json!({ "native_tokens_prompt": null }),
            json!({ "native_tokens_completion": -1 }),
            json!({ "native_tokens_cached": 1.5 }),
            json!({ "billable_web_search_calls": null }),
        ] {
            assert_eq!(parse(&record(changes.clone()), ID), None, "{changes}");
        }
        assert_eq!(parse(b"not json", ID), None);
        assert_eq!(parse(br#"{"id":"x"}"#, ID), None);
        assert_eq!(parse(&record(json!({})), other), None);
    }

    #[test]
    fn a_status_says_whether_to_ask_again() {
        for status in [404, 408, 425, 429, 500, 502, 503, 599] {
            assert_eq!(classify(status), Lookup::Retry, "{status}");
        }
        for status in [401, 403] {
            assert_eq!(classify(status), Lookup::Preserve, "{status}");
        }
        for status in [301, 400, 410, 422] {
            assert_eq!(classify(status), Lookup::Reject, "{status}");
        }
    }

    #[test]
    fn only_the_gateway_and_a_loopback_server_are_asked() {
        for origin in [
            "https://ai-gateway.vercel.sh",
            "http://127.0.0.1:4100",
            "http://localhost:4100",
            "http://[::1]:4100",
        ] {
            assert!(is_trusted_generation_origin(origin), "{origin}");
        }
        for origin in [
            "",
            "https://gateway.example.com",
            "https://ai-gateway.vercel.sh.example.com",
            "http://ai-gateway.vercel.sh",
            "http://127.0.0.1",
            "http://192.168.1.2:4100",
            "https://127.0.0.1:4100",
            "http://user@127.0.0.1:4100",
        ] {
            assert!(!is_trusted_generation_origin(origin), "{origin}");
        }
    }

    #[test]
    fn an_untrusted_origin_is_rejected_without_a_socket() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            runtime.block_on(lookup("https://gateway.example.com", "key", ID)),
            Lookup::Reject
        );
        assert_eq!(
            runtime.block_on(lookup("http://127.0.0.1:1", "key", "gen_bad")),
            Lookup::Reject
        );
    }
}
