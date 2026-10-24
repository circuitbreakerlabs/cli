pub(crate) fn masked_api_key_help(description: &str, env_name: &str) -> String {
    let value = std::env::var(env_name).ok();
    let masked = mask_api_key(value.as_deref());
    format!("{description} [env: {env_name}={masked}]")
}

fn mask_api_key(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };

    let mut characters = value.chars().rev();
    let suffix: Vec<_> = characters.by_ref().take(4).collect();
    if characters.next().is_some() {
        format!("****{}", suffix.into_iter().rev().collect::<String>())
    } else {
        "****".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::mask_api_key;

    #[test]
    fn masks_keys_without_splitting_unicode_characters() {
        for (value, expected) in [
            (None, ""),
            (Some(""), "****"),
            (Some("abc"), "****"),
            (Some("abcd"), "****"),
            (Some("abcde"), "****bcde"),
            (Some("test-secret-1234"), "****1234"),
            (Some("é🔑字"), "****"),
            (Some("aé🔑字"), "****"),
            (Some("secret-aé🔑字"), "****aé🔑字"),
        ] {
            assert_eq!(mask_api_key(value), expected);
        }
    }
}
