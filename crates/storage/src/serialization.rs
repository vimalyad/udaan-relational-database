use core::error::{CrdtError, CrdtResult};

/// Serialize a value to canonical CBOR bytes.
pub fn to_cbor<T: serde::Serialize>(value: &T) -> CrdtResult<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|e| CrdtError::SerializationError(e.to_string()))?;
    Ok(buf)
}

/// Deserialize a value from CBOR bytes.
pub fn from_cbor<T: serde::de::DeserializeOwned>(data: &[u8]) -> CrdtResult<T> {
    ciborium::from_reader(data).map_err(|e| CrdtError::SerializationError(e.to_string()))
}

/// Serialize to JSON (for Python adapter bridge).
pub fn to_json<T: serde::Serialize>(value: &T) -> CrdtResult<String> {
    serde_json::to_string(value).map_err(|e| CrdtError::SerializationError(e.to_string()))
}

/// Deserialize from JSON.
pub fn from_json<T: serde::de::DeserializeOwned>(data: &str) -> CrdtResult<T> {
    serde_json::from_str(data).map_err(|e| CrdtError::SerializationError(e.to_string()))
}
