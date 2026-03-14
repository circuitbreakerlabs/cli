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

#[cfg(test)]
mod tests {
    use super::{Headers, merge_headers};
    use reqwest::header::HeaderMap;
    use std::str::FromStr;

    #[test]
    fn parses_header_and_trims_whitespace() {
        let headers = Headers::from_str(" X-Test :  value  ").expect("header should parse");
        let header_map: HeaderMap = headers.into();

        assert_eq!(header_map["x-test"], "value");
    }

    #[test]
    fn rejects_header_without_separator() {
        let err = Headers::from_str("X-Test").expect_err("header should fail to parse");

        assert!(err.contains("Expected 'Key:Value'"));
    }

    #[test]
    fn merge_headers_combines_multiple_maps() {
        let headers = vec![
            Headers::from_str("X-One:1").expect("first header should parse"),
            Headers::from_str("X-Two:2").expect("second header should parse"),
        ];

        let merged = merge_headers(&headers);

        assert_eq!(merged["x-one"], "1");
        assert_eq!(merged["x-two"], "2");
    }
}
