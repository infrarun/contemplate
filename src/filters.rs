use base64::Engine as _;
use hex::ToHex;
use minijinja::value::{Value, ValueKind};
use minijinja::{Environment, Error, ErrorKind};

pub fn register(env: &mut Environment) {
    env.add_filter("base64encode", base64encode);
    env.add_filter("hexencode", hexencode);
    env.add_filter("from_json", from_json);
    #[cfg(feature = "yaml")]
    env.add_filter("from_yaml", from_yaml);
    #[cfg(feature = "toml")]
    env.add_filter("from_toml", from_toml);
}

fn value_as_bytes(value: &Value) -> Result<Vec<u8>, Error> {
    if let Some(string) = value.as_str() {
        return Ok(string.as_bytes().into());
    }

    if matches!(value.kind(), ValueKind::Seq | ValueKind::Iterable) {
        let bytes = value
            .try_iter()?
            .map(|it| {
                if it.is_number() {
                    u8::try_from(it).map_err(|_| {
                        Error::new(ErrorKind::InvalidOperation, "Invalid sequence (not u8)!")
                    })
                } else {
                    Err(Error::new(
                        ErrorKind::InvalidOperation,
                        "Invalid sequence (not numeric)!",
                    ))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(bytes);
    }

    if let Some(bytes) = value.as_bytes() {
        return Ok(bytes.into());
    };

    Err(Error::new(
        ErrorKind::InvalidOperation,
        "Invalid type (can't get byte stream)",
    ))
}

pub fn base64encode(value: &Value) -> Result<String, Error> {
    value_as_bytes(value).map(|v| base64::engine::general_purpose::STANDARD.encode(v))
}

pub fn hexencode(value: &Value) -> Result<String, Error> {
    let bytes = value_as_bytes(value)?;

    Ok(bytes.as_slice().encode_hex())
}

pub fn from_json(value: &Value) -> Result<Value, Error> {
    let Some(value) = value.as_str() else {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "from_json requires a string input",
        ));
    };

    let value: serde_json::Value = serde_json::from_str(value).map_err(|e| {
        Error::new(
            ErrorKind::BadSerialization,
            format!("Could not deserialize: {e}"),
        )
    })?;

    let value = Value::from_serialize(value);
    Ok(value)
}

#[cfg(feature = "yaml")]
pub fn from_yaml(value: &Value) -> Result<Value, Error> {
    let Some(value) = value.as_str() else {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "from_yaml requires a string input",
        ));
    };

    let value: serde_yaml::Value = serde_yaml::from_str(value).map_err(|e| {
        Error::new(
            ErrorKind::BadSerialization,
            format!("Could not deserialize: {e}"),
        )
    })?;

    let value = Value::from_serialize(value);
    Ok(value)
}

#[cfg(feature = "toml")]
pub fn from_toml(value: &Value) -> Result<Value, Error> {
    let Some(value) = value.as_str() else {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "from_toml requires a string input",
        ));
    };

    let value: toml::Value = value.parse().map_err(|e| {
        Error::new(
            ErrorKind::BadSerialization,
            format!("Could not deserialize: {e}"),
        )
    })?;

    let value = Value::from_serialize(value);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64encode_string() {
        let value = Value::from_safe_string(String::from("hello"));
        let base64 = base64encode(&value).unwrap();
        assert_eq!(base64, "aGVsbG8=")
    }

    #[test]
    fn test_base64encode_byte_seq() {
        let value = Value::from_serialize(&[1, 2, 3, 4]);
        let base64 = base64encode(&value).unwrap();
        assert_eq!(base64, "AQIDBA==")
    }

    #[test]
    fn test_base64encode_non_byte_seq() {
        let value = Value::from_serialize(&[257]);
        let error = base64encode(&value).unwrap_err();
        assert_eq!(error.kind(), minijinja::ErrorKind::InvalidOperation);
        assert_eq!(error.detail(), Some("Invalid sequence (not u8)!"));
    }

    #[test]
    fn test_base64encode_non_numeric_seq() {
        let value = Value::from_serialize(&["hello"]);
        let error = base64encode(&value).unwrap_err();
        assert_eq!(error.kind(), minijinja::ErrorKind::InvalidOperation);
        assert_eq!(error.detail(), Some("Invalid sequence (not numeric)!"));
    }

    #[test]
    fn test_base64encode_boolean() {
        let value = Value::from_serialize(false);
        let error = base64encode(&value).unwrap_err();
        assert_eq!(error.kind(), minijinja::ErrorKind::InvalidOperation);
        assert_eq!(error.detail(), Some("Invalid type (can't get byte stream)"));
    }

    #[test]
    fn test_hexencode_string() {
        let value = Value::from_safe_string("Hello".into());
        let hex = hexencode(&value).unwrap();
        assert_eq!(hex, "48656c6c6f");
    }

    #[test]
    fn test_hexencode_bytes() {
        let value = Value::from_serialize(&[1, 2, 3, 4]);
        let hex = hexencode(&value).unwrap();
        assert_eq!(hex, "01020304");
    }

    #[test]
    fn test_hexencode_non_byte_seq() {
        let value = Value::from_serialize(&[257]);
        let error = hexencode(&value).unwrap_err();
        assert_eq!(error.kind(), minijinja::ErrorKind::InvalidOperation);
        assert_eq!(error.detail(), Some("Invalid sequence (not u8)!"));
    }

    #[test]
    fn test_hexencode_non_numeric_seq() {
        let value = Value::from_serialize(&["hello"]);
        let error = hexencode(&value).unwrap_err();
        assert_eq!(error.kind(), minijinja::ErrorKind::InvalidOperation);
        assert_eq!(error.detail(), Some("Invalid sequence (not numeric)!"));
    }

    #[test]
    fn test_hexencode_boolean() {
        let value = Value::from_serialize(false);
        let error = hexencode(&value).unwrap_err();
        assert_eq!(error.kind(), minijinja::ErrorKind::InvalidOperation);
        assert_eq!(error.detail(), Some("Invalid type (can't get byte stream)"));
    }
}
