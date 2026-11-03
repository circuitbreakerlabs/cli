use std::time::{Duration, SystemTime};

use reqwest::header::{HeaderMap, RETRY_AFTER};

use super::{ProviderCompletion, ProviderError};
use crate::protocol_types::common::CompletionErrorReason;

pub fn parse_retry_after(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    headers
        .get_all(RETRY_AFTER)
        .iter()
        .filter_map(|value| {
            let value = value.to_str().ok()?.trim();
            let delay = if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
                Duration::from_secs(value.parse().ok()?)
            } else {
                httpdate::parse_http_date(value)
                    .ok()?
                    .duration_since(now)
                    .unwrap_or_default()
            };
            // Reject delays that cannot be represented by the timer implementation.
            std::time::Instant::now().checked_add(delay)?;
            Some(delay)
        })
        .max()
}

pub(super) async fn run<F, Fut>(mut attempt: F) -> Result<ProviderCompletion, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<ProviderCompletion, ProviderError>>,
{
    let mut delays = [2, 4, 8].into_iter().enumerate();
    loop {
        let error = match attempt().await {
            Ok(completion) => return Ok(completion),
            Err(error) => error,
        };
        if !matches!(
            CompletionErrorReason::from(&error),
            CompletionErrorReason::RateLimited
        ) {
            return Err(error);
        }
        let Some((index, seconds)) = delays.next() else {
            return Err(error);
        };
        let delay = Duration::from_secs(seconds).max(error.retry_after().unwrap_or_default());
        tracing::warn!(
            retry = index + 1,
            delay_seconds = delay.as_secs_f64(),
            "Target rate limited; scheduling retry"
        );
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol_types::Message;
    use crate::response_provider::ResponseProvider;
    use async_trait::async_trait;
    use std::{collections::VecDeque, sync::Mutex};
    use tokio::time::Instant;

    struct ScriptedProvider {
        outcomes: Mutex<VecDeque<Result<ProviderCompletion, ProviderError>>>,
        calls: Mutex<Vec<Instant>>,
    }

    impl ScriptedProvider {
        fn new(outcomes: Vec<Result<ProviderCompletion, ProviderError>>) -> Self {
            Self {
                outcomes: Mutex::new(outcomes.into()),
                calls: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl ResponseProvider for ScriptedProvider {
        async fn generate_response(
            &self,
            messages: &[Message],
        ) -> Result<ProviderCompletion, ProviderError> {
            assert_eq!(messages[0].content, "unchanged");
            self.calls.lock().unwrap().push(Instant::now());
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra attempt")
        }
    }

    fn history() -> Vec<Message> {
        vec![Message {
            name: None,
            role: crate::protocol_types::Role::User,
            content: "unchanged".into(),
        }]
    }

    fn limited(seconds: Option<u64>) -> Result<ProviderCompletion, ProviderError> {
        Err(ProviderError::RateLimited {
            message: "busy".into(),
            retry_after: seconds.map(Duration::from_secs),
        })
    }

    fn success() -> ProviderCompletion {
        ProviderCompletion {
            message: Message {
                name: None,
                role: crate::protocol_types::Role::Assistant,
                content: "reply".into(),
            },
            model_id: Some("model".into()),
            tokens_used: Some(42),
            finish_reason: Some("stop".into()),
            provider_response_id: Some("response-id".into()),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_on_each_attempt_and_preserves_metadata() {
        for retries in 0..=3 {
            let mut outcomes = (0..retries).map(|_| limited(None)).collect::<Vec<_>>();
            outcomes.push(Ok(success()));
            let provider = ScriptedProvider::new(outcomes);
            let messages = history();
            let start = Instant::now();
            let result = run(|| provider.generate_response(&messages)).await.unwrap();
            assert_eq!(result.message.content, "reply");
            assert_eq!(result.model_id.as_deref(), Some("model"));
            assert_eq!(result.tokens_used, Some(42));
            assert_eq!(result.finish_reason.as_deref(), Some("stop"));
            assert_eq!(result.provider_response_id.as_deref(), Some("response-id"));
            let calls = provider.calls.lock().unwrap();
            assert_eq!(calls.len(), retries + 1);
            for (call, seconds) in calls.iter().zip([0, 2, 6, 14]) {
                assert_eq!(*call - start, Duration::from_secs(seconds));
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn exhausts_budget_and_honors_only_longer_delays_without_resetting_schedule() {
        let provider = ScriptedProvider::new(vec![
            limited(Some(120)),
            limited(Some(1)),
            limited(None),
            limited(Some(900)),
        ]);
        let messages = history();
        let start = Instant::now();
        let error = run(|| provider.generate_response(&messages))
            .await
            .unwrap_err();
        assert_eq!(error.retry_after(), Some(Duration::from_mins(15)));
        for (call, seconds) in provider
            .calls
            .lock()
            .unwrap()
            .iter()
            .zip([0, 120, 124, 132])
        {
            assert_eq!(*call - start, Duration::from_secs(seconds));
        }
        assert_eq!(provider.calls.lock().unwrap().len(), 4);
        assert_eq!(Instant::now() - start, Duration::from_secs(132));
    }

    #[tokio::test(start_paused = true)]
    async fn stops_on_other_errors_even_after_rate_limit() {
        for prior_limit in [false, true] {
            let mut outcomes = vec![];
            if prior_limit {
                outcomes.push(limited(None));
            }
            outcomes.push(Err(ProviderError::Network("offline".into())));
            let provider = ScriptedProvider::new(outcomes);
            let messages = history();
            assert!(matches!(
                run(|| provider.generate_response(&messages)).await,
                Err(ProviderError::Network(_))
            ));
            assert_eq!(
                provider.calls.lock().unwrap().len(),
                if prior_limit { 2 } else { 1 }
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_requests_are_independent_and_waits_can_be_cancelled() {
        let slow = std::sync::Arc::new(ScriptedProvider::new(vec![
            limited(Some(3600)),
            Ok(success()),
        ]));
        let clone = slow.clone();
        let task = tokio::spawn(async move {
            let messages = history();
            run(|| clone.generate_response(&messages)).await
        });
        tokio::task::yield_now().await;
        let fast = ScriptedProvider::new(vec![Ok(success())]);
        let messages = history();
        run(|| fast.generate_response(&messages)).await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::advance(Duration::from_hours(2)).await;
        assert_eq!(slow.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn parses_seconds_dates_and_repeated_headers() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let future = httpdate::fmt_http_date(now + Duration::from_secs(100));
        let past = httpdate::fmt_http_date(now - Duration::from_secs(100));
        for (value, expected) in [
            ("12", Some(12)),
            (" 12 ", Some(12)),
            ("0", Some(0)),
            ("-1", None),
            ("1.5", None),
            ("+2", None),
            ("", None),
            ("invalid", None),
            ("18446744073709551616", None),
            (future.as_str(), Some(100)),
            (past.as_str(), Some(0)),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, value.parse().unwrap());
            assert_eq!(
                parse_retry_after(&headers, now),
                expected.map(Duration::from_secs),
                "{value}"
            );
        }
        let mut headers = HeaderMap::new();
        assert_eq!(parse_retry_after(&headers, now), None);
        for value in ["invalid", "12", &future, "30"] {
            headers.append(RETRY_AFTER, value.parse().unwrap());
        }
        assert_eq!(
            parse_retry_after(&headers, now),
            Some(Duration::from_secs(100))
        );
    }
}
