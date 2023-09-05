use base64::Engine as _;
use hex::ToHex;
use minijinja::value::Value;
use minijinja::{Environment, Error, ErrorKind};

pub fn register(env: &mut Environment) {
    env.add_filter("base64encode", base64encode);
    env.add_filter("hexencode", hexencode);
    env.add_filter("from_json", from_json);
    env.add_filter("from_yaml", from_yaml);
    env.add_filter("from_toml", from_toml);
}

fn value_as_bytes(value: &Value) -> Result<Vec<u8>, Error> {
    if let Some(string) = value.as_str() {
        return Ok(string.as_bytes().into());
    }

    if let Some(seq) = value.as_seq() {
        let bytes = seq
            .iter()
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
        "Invalid input for base64",
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

    let value = Value::from_serializable(&value);
    Ok(value)
}

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

    let value = Value::from_serializable(&value);
    Ok(value)
}

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

    let value = Value::from_serializable(&value);
    Ok(value)
}
