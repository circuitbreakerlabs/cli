use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::multiturn::run_evaluation;
use super::test_support::{recv_text_json, send_json, spawn_websocket_server};
use crate::protocol_types::common::RerunSelector;
use crate::protocol_types::multi_turn::{
    MultiTurnEvaluationRequest, MultiTurnRequest, MultiTurnRerunRequest,
};
use crate::response_provider::test_support::{read_request, serve_requests};
use crate::response_provider::{CustomProvider, CustomProviderConfig, HttpResponseProvider};

const RAW_REPLY: &str = "“I hear you.”\n[EXERCISE]\nLet’s talk.";
const FINAL_REPLY: &str = "Take care. [SESSION ENDED]\n";

fn completion(id: i32, request_id: &str, messages: &Value) -> Value {
    json!({
        "type": "completion_request",
        "data": {"request_id": request_id, "conversation_id": id, "messages": messages},
    })
}

fn initial(id: i32, request_id: &str, content: &str) -> Value {
    completion(
        id,
        request_id,
        &json!([
            {"role": "system", "content": "Test prompt"},
            {"role": "user", "content": content, "name": "Test name"},
        ]),
    )
}

fn finish(id: i32) -> Value {
    json!({"type": "conversation_complete", "data": {"conversation_id": id, "turns": 2, "passed": true}})
}

fn error(id: i32) -> Value {
    json!({"type": "conversation_error", "data": {"conversation_id": id, "error_message": "Test error"}})
}

fn mock_reply(content: &str) -> (u16, Option<String>, String) {
    (
        200,
        None,
        json!({
            "choices": [{"message": {"content": content}, "finish_reason": "stop"}],
            "usage": {"total_tokens": 0},
            "cogo_greeting": "Welcome!",
        })
        .to_string(),
    )
}

async fn mock_session_endpoint() -> (String, tokio::task::JoinHandle<Vec<(String, Value)>>) {
    let mut history = HashMap::new();
    let mut rate_limited_body = None;
    serve_requests(9, move |headers, body| {
        assert!(
            headers
                .lines()
                .any(|line| line == "authorization: Bearer test-key")
        );
        if headers.starts_with("DELETE ") {
            let id = headers
                .split_whitespace()
                .nth(1)
                .unwrap()
                .strip_prefix("/v1/conversations/")
                .unwrap();
            let status = if history.remove(id).is_some() {
                204
            } else {
                410
            };
            return (status, None, String::new());
        }

        assert!(headers.starts_with("POST /v1/chat/completions "));
        assert_eq!(body["model"], "cogo-solo");
        assert_eq!(body["stream"], false);
        let id = body["metadata"]["conversation_id"].as_str().unwrap();
        assert_eq!(body["metadata"], json!({"conversation_id": id}));
        let messages = body["messages"].as_array().unwrap();
        let input = messages.last().unwrap()["content"].as_str().unwrap();
        if input == "expired" {
            assert_eq!(id, "cbl-expired");
            return (410, None, "{}".into());
        }

        let previous = history.entry(id.to_owned()).or_insert_with(Vec::new);
        if &messages[..messages.len() - 1] != previous.as_slice() {
            return (409, None, "{}".into());
        }
        assert_eq!(
            messages.last().unwrap(),
            &json!({"role": "user", "content": input})
        );
        if input == "first A" {
            assert_eq!(id, "cbl-first-a");
            if let Some(first_body) = &rate_limited_body {
                assert_eq!(body, first_body);
            } else {
                rate_limited_body = Some(body.clone());
                return (429, Some("3".into()), "{}".into());
            }
        }
        let reply = match input {
            "first A" => RAW_REPLY,
            "first B" => {
                assert_eq!(id, "cbl-first-b");
                "Reply B"
            }
            "continue" => {
                assert_eq!(id, "cbl-first-a");
                FINAL_REPLY
            }
            "rerun A" => {
                assert_eq!(id, "cbl-rerun-a");
                "Rerun reply"
            }
            other => panic!("Unexpected user input {other}"),
        };
        previous.push(messages.last().unwrap().clone());
        previous.push(json!({"role": "assistant", "content": reply}));
        mock_reply(reply)
    })
    .await
}

fn session_provider(base_url: &str) -> Arc<HttpResponseProvider> {
    let api = CustomProvider::new(&CustomProviderConfig {
        url: format!("{base_url}/v1/chat/completions"),
        script: Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/providers/cogo_solo.rhai"),
    })
    .unwrap();
    let headers =
        HeaderMap::from_iter([(AUTHORIZATION, HeaderValue::from_static("Bearer test-key"))]);
    Arc::new(HttpResponseProvider::new(Box::new(api), &headers).unwrap())
}

fn standard_request() -> MultiTurnEvaluationRequest {
    MultiTurnEvaluationRequest::Standard(MultiTurnRequest {
        threshold: 0.5,
        max_turns: 4,
        test_case_groups: vec![],
    })
}

#[tokio::test]
async fn multiturn_replays_exact_history_and_closes_sessions() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let (base_url, endpoint) = mock_session_endpoint().await;
        let provider = session_provider(&base_url);
        run_standard(provider.clone()).await;
        run_rerun(provider).await;

        let requests = endpoint.await.unwrap();
        let mut deletes = requests
            .iter()
            .filter_map(|(header, _)| header.strip_prefix("delete "))
            .map(|header| header.split_whitespace().next().unwrap())
            .collect::<Vec<_>>();
        deletes.sort_unstable();
        assert_eq!(
            deletes,
            [
                "/v1/conversations/cbl-expired",
                "/v1/conversations/cbl-first-b",
                "/v1/conversations/cbl-rerun-a",
            ]
        );
    })
    .await
    .unwrap();
}

async fn run_standard(provider: Arc<HttpResponseProvider>) {
    let (socket, server) = spawn_websocket_server(|socket| async move {
        let (mut write, mut read) = socket.split();
        assert_eq!(
            recv_text_json(&mut read).await["type"],
            "multi_turn_request"
        );
        send_json(&mut write, initial(0, "first-a", "first A")).await;
        send_json(&mut write, initial(1, "first-b", "first B")).await;
        let mut replies = HashMap::new();
        for _ in 0..2 {
            let reply = recv_text_json(&mut read).await;
            assert_eq!(reply["type"], "completion_response");
            replies.insert(
                reply["data"]["request_id"].as_str().unwrap().to_owned(),
                reply,
            );
        }
        assert_eq!(replies["first-a"]["data"]["model_response"], RAW_REPLY);
        assert_eq!(replies["first-b"]["data"]["model_response"], "Reply B");
        send_json(&mut write, finish(1)).await;
        send_json(&mut write, error(1)).await;
        let history = json!([
            {"role": "system", "content": "Test prompt"},
            {"role": "user", "content": "first A"},
            {"role": "assistant", "content": "\"I hear you.\" [EXERCISE] Let's talk."},
            {"role": "user", "content": "continue"},
        ]);
        send_json(&mut write, completion(0, "second-a", &history)).await;
        let reply = recv_text_json(&mut read).await;
        assert_eq!(reply["data"]["request_id"], "second-a");
        assert_eq!(reply["data"]["model_response"], FINAL_REPLY);
        assert_eq!(reply["data"]["finish_reason"], "session_ended");
        send_json(&mut write, initial(0, "after-provider-close", "unexpected")).await;
        let reply = recv_text_json(&mut read).await;
        assert_eq!(reply["type"], "completion_error");
        assert_eq!(reply["data"]["request_id"], "after-provider-close");
        send_json(&mut write, finish(0)).await;
        send_json(
            &mut write,
            json!({
                "type": "multi_turn_response",
                "data": {"evaluation_id": 1, "total_passed": 2, "total_failed": 0},
            }),
        )
        .await;
    })
    .await;
    let result = run_evaluation(socket, provider, standard_request(), 2, None)
        .await
        .unwrap();
    assert_eq!(result.total_passed, 2);
    server.await.unwrap();
}

async fn run_rerun(provider: Arc<HttpResponseProvider>) {
    let (socket, server) = spawn_websocket_server(|socket| async move {
        let (mut write, mut read) = socket.split();
        assert_eq!(
            recv_text_json(&mut read).await["type"],
            "multi_turn_rerun_request"
        );
        send_json(&mut write, initial(1, "expired", "expired")).await;
        let reply = recv_text_json(&mut read).await;
        assert_eq!(reply["type"], "completion_error");
        assert_eq!(reply["data"]["request_id"], "expired");
        send_json(&mut write, error(1)).await;
        send_json(&mut write, initial(0, "rerun-a", "rerun A")).await;
        assert_eq!(
            recv_text_json(&mut read).await["data"]["model_response"],
            "Rerun reply"
        );
        write.close().await.unwrap();
    })
    .await;
    let result = run_evaluation(
        socket,
        provider,
        MultiTurnEvaluationRequest::Rerun(MultiTurnRerunRequest {
            selector: RerunSelector::TestResultIds {
                test_result_ids: vec![1],
            },
            threshold: 0.5,
            max_turns: 4,
        }),
        2,
        None,
    )
    .await;
    assert!(matches!(
        result,
        Err(super::EvaluationError::WebSocketClosed(_))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn conversation_error_cancels_the_retry_before_cleanup() {
    let (post_received, wait_for_post) = tokio::sync::oneshot::channel();
    let (delete_received, wait_for_delete) = tokio::sync::oneshot::channel();
    let mut post_received = Some(post_received);
    let mut delete_received = Some(delete_received);
    let (base_url, endpoint) = serve_requests(2, move |headers, _| {
        if headers.starts_with("POST ") {
            post_received.take().unwrap().send(()).unwrap();
            (429, Some("60".into()), "{}".into())
        } else {
            assert!(headers.starts_with("DELETE /v1/conversations/cbl-limited "));
            delete_received.take().unwrap().send(()).unwrap();
            (404, None, String::new())
        }
    })
    .await;
    let (socket, server) = spawn_websocket_server(|socket| async move {
        let (mut write, mut read) = socket.split();
        let _ = recv_text_json(&mut read).await;
        send_json(&mut write, initial(0, "limited", "initial")).await;
        wait_for_post.await.unwrap();
        send_json(&mut write, initial(0, "busy", "unexpected")).await;
        let reply = recv_text_json(&mut read).await;
        assert_eq!(reply["type"], "completion_error");
        assert_eq!(reply["data"]["request_id"], "busy");
        send_json(&mut write, initial(1, "queued", "queued")).await;
        send_json(&mut write, initial(1, "already-queued", "unexpected")).await;
        let reply = recv_text_json(&mut read).await;
        assert_eq!(reply["type"], "completion_error");
        assert_eq!(reply["data"]["request_id"], "already-queued");
        send_json(&mut write, error(1)).await;
        send_json(&mut write, error(0)).await;
        wait_for_delete.await.unwrap();
        send_json(
            &mut write,
            json!({
                "type": "multi_turn_response",
                "data": {"evaluation_id": 2, "total_passed": 0, "total_failed": 1},
            }),
        )
        .await;
    })
    .await;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        run_evaluation(
            socket,
            session_provider(&base_url),
            standard_request(),
            1,
            None,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.total_failed, 1);
    server.await.unwrap();
    endpoint.await.unwrap();
}

#[tokio::test]
async fn conversation_error_waits_for_the_in_flight_post_before_cleanup() {
    tokio::time::timeout(Duration::from_secs(10), verify_post_cleanup_order(false))
        .await
        .unwrap();
}

#[tokio::test]
async fn disconnect_waits_for_the_in_flight_post_before_cleanup() {
    tokio::time::timeout(Duration::from_secs(10), verify_post_cleanup_order(true))
        .await
        .unwrap();
}

async fn verify_post_cleanup_order(disconnect: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let (post_received, wait_for_post) = tokio::sync::oneshot::channel();
    let (case_ended, wait_for_end) = tokio::sync::oneshot::channel();
    let (session_deleted, wait_for_delete) = tokio::sync::oneshot::channel();
    let endpoint = tokio::spawn(async move {
        let (mut post, _) = listener.accept().await.unwrap();
        let (headers, request) = read_request(&mut post).await;
        assert!(headers.starts_with("POST /v1/chat/completions "));
        assert_eq!(request["metadata"]["conversation_id"], "cbl-first-post");
        post_received.send(()).unwrap();
        wait_for_end.await.unwrap();

        assert!(
            tokio::time::timeout(Duration::from_millis(250), listener.accept())
                .await
                .is_err(),
            "cleanup arrived before the first POST could open the session"
        );
        let (_, _, body) = mock_reply("Late reply");
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        post.write_all(reply.as_bytes()).await.unwrap();

        let (mut delete, _) = listener.accept().await.unwrap();
        let (headers, _) = read_request(&mut delete).await;
        assert!(headers.starts_with("DELETE /v1/conversations/cbl-first-post "));
        delete
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let _ = session_deleted.send(());
    });

    let (socket, server) = spawn_websocket_server(move |socket| async move {
        let (mut write, mut read) = socket.split();
        let _ = recv_text_json(&mut read).await;
        send_json(&mut write, initial(0, "first-post", "initial")).await;
        wait_for_post.await.unwrap();
        if disconnect {
            write.close().await.unwrap();
            case_ended.send(()).unwrap();
            return;
        }

        send_json(&mut write, error(0)).await;
        send_json(&mut write, initial(0, "closed", "unexpected")).await;
        let reply = recv_text_json(&mut read).await;
        assert_eq!(reply["type"], "completion_error");
        assert_eq!(reply["data"]["request_id"], "closed");
        case_ended.send(()).unwrap();
        wait_for_delete.await.unwrap();
        send_json(&mut write, initial(0, "still-closed", "unexpected")).await;
        assert_eq!(
            recv_text_json(&mut read).await["data"]["request_id"],
            "still-closed"
        );
        send_json(
            &mut write,
            json!({
                "type": "multi_turn_response",
                "data": {"evaluation_id": 3, "total_passed": 0, "total_failed": 1},
            }),
        )
        .await;
    })
    .await;
    let result = run_evaluation(
        socket,
        session_provider(&base_url),
        standard_request(),
        1,
        None,
    )
    .await;
    if disconnect {
        assert!(matches!(
            result,
            Err(super::EvaluationError::WebSocketClosed(_))
        ));
    } else {
        assert_eq!(result.unwrap().total_failed, 1);
    }
    server.await.unwrap();
    endpoint.await.unwrap();
}
