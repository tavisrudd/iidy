use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_yaml::Value;

/// Placeholder for the YAML preprocessing system from `iidy-js`.
///
/// For now this function simply deserializes the provided `Value` into the
/// requested type without any transformation.
pub fn preprocess<T: DeserializeOwned>(value: Value) -> Result<T> {
    // TODO: implement the full preprocessing language from iidy-js
    Ok(serde_yaml::from_value(value)?)
}
