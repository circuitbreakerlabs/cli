use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;

#[derive(Clone, Debug, Default)]
pub struct Headers(pub HeaderMap);

impl FromStr for Headers {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (name, value) = s
            .split_once(':')
            .ok_or_else(|| format!("invalid header format '{s}'. Expected 'Key:Value'"))?;
        let name = name.trim();
        let value = value.trim();

        let header_name =
            HeaderName::try_from(name).map_err(|e| format!("invalid header name '{name}': {e}"))?;
        let header_value = HeaderValue::try_from(value)
            .map_err(|e| format!("invalid header value '{value}': {e}"))?;

        Ok(Headers(HeaderMap::from_iter([(header_name, header_value)])))
    }
}

impl From<Headers> for HeaderMap {
    fn from(headers: Headers) -> HeaderMap {
        headers.0
    }
}

pub fn merge_headers(headers: &[Headers]) -> HeaderMap {
    headers
        .iter()
        .flat_map(|h| h.0.iter())
        .map(|(n, v)| (n.clone(), v.clone()))
        .collect()
}
