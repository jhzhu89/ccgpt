use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;

const PREFIX: &str = "ccgpt:";

pub fn sanitize_reasoning_item(item: &Value) -> Option<Value> {
    let mut object = item.as_object()?.clone();
    if object.get("type").and_then(Value::as_str) != Some("reasoning")
        || !object
            .get("encrypted_content")
            .is_some_and(Value::is_string)
        || !object.get("summary").is_some_and(Value::is_array)
    {
        return None;
    }
    object.remove("id");
    object.remove("status");
    Some(Value::Object(object))
}

pub fn encode_signature(item: &Value) -> Option<String> {
    sanitize_reasoning_item(item)?;
    let json = serde_json::to_vec(item).ok()?;
    Some(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(json)))
}

pub fn decode_signature(signature: &str) -> Option<Value> {
    let encoded = signature.strip_prefix(PREFIX)?;
    let json = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    let item = serde_json::from_slice(&json).ok()?;
    sanitize_reasoning_item(&item)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{decode_signature, encode_signature, sanitize_reasoning_item};

    #[test]
    fn round_trips_and_sanitizes_a_reasoning_item() {
        let item = json!({
            "id": "rs_123",
            "type": "reasoning",
            "status": "completed",
            "summary": [{ "type": "summary_text", "text": "summary" }],
            "encrypted_content": "encrypted",
            "future_field": { "keep": true }
        });

        let signature = encode_signature(&item).unwrap();
        assert!(signature.starts_with("ccgpt:"));
        assert_eq!(
            decode_signature(&signature),
            Some(json!({
                "type": "reasoning",
                "summary": [{ "type": "summary_text", "text": "summary" }],
                "encrypted_content": "encrypted",
                "future_field": { "keep": true }
            }))
        );
    }

    #[test]
    fn rejects_foreign_or_invalid_signatures() {
        assert_eq!(decode_signature("foreign:abc"), None);
        assert_eq!(decode_signature("ccgpt:not-base64!"), None);

        let wrong_type = json!({
            "type": "message",
            "summary": [],
            "encrypted_content": "encrypted"
        });
        let missing_encrypted = json!({ "type": "reasoning", "summary": [] });
        let invalid_summary = json!({
            "type": "reasoning",
            "summary": "summary",
            "encrypted_content": "encrypted"
        });

        assert_eq!(encode_signature(&wrong_type), None);
        assert_eq!(encode_signature(&missing_encrypted), None);
        assert_eq!(encode_signature(&invalid_summary), None);
    }

    #[test]
    fn sanitizes_replay_without_mutating_the_source() {
        let item = json!({
            "id": "rs_123",
            "type": "reasoning",
            "status": "completed",
            "summary": [],
            "encrypted_content": "encrypted"
        });

        let replay = sanitize_reasoning_item(&item).unwrap();
        assert!(replay.get("id").is_none());
        assert!(replay.get("status").is_none());
        assert_eq!(item["id"], "rs_123");
        assert_eq!(item["status"], "completed");
    }
}
