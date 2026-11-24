fn conversation_id(context) {
    "cbl-" + context.first_request_id
}

fn build_request(messages, context) {
    let history = [];
    let assistant_index = 0;
    let user_count = 0;
    let assistant_replies = context.assistant_replies;
    let assistant_count = assistant_replies.len;
    for message in messages {
        let role = message.role;
        let content = message.content;
        if role == "user" {
            user_count += 1;
            history.push(#{role: role, content: content});
        }
        if role == "assistant" {
            if assistant_index >= assistant_count {
                throw "Missing raw CoGo assistant reply for replay";
            }
            let raw_reply = assistant_replies[assistant_index];
            history.push(#{role: role, content: raw_reply});
            assistant_index += 1;
        }
    }
    if assistant_index != assistant_count || history.is_empty() {
        throw "Invalid CoGo conversation history";
    }
    let last_message = history[-1];
    if last_message.role != "user" {
        throw "Invalid CoGo conversation history";
    }
    if user_count > 30 {
        throw "CoGo allows at most 30 user messages per conversation";
    }
    let metadata = #{conversation_id: conversation_id(context)};
    #{
        model: "cogo-solo",
        stream: false,
        messages: history,
        metadata: metadata
    }
}

fn parse_response(response) {
    let status = response.status;
    if status < 200 || status >= 300 {
        let reason = "unknown";
        if status == 401 { reason = "authentication_failed"; }
        else if status == 409 { reason = "invalid_response"; }
        else if status == 429 { reason = "rate_limited"; }
        else if status == 502 { reason = "model_unreachable"; }
        else if status == 504 { reason = "model_timeout"; }
        return #{type: "error", reason: reason, message: "CoGo HTTP " + status.to_string()};
    }
    try {
        let body = response.body;
        let choice = body.choices[0];
        let content = choice.message.content;
        if type_of(content) == "string" && content != "" {
            let result = #{type: "success", content: content};
            let trimmed = content;
            trimmed.trim();
            if trimmed.ends_with("[SESSION ENDED]") {
                result.finish_reason = "session_ended";
            }
            return result;
        }
    } catch (error) {
        // Missing fields follow the same error path as malformed content.
    }
    #{type: "error", reason: "invalid_response", message: "Missing or invalid CoGo assistant content"}
}

fn build_close_path(context) {
    "/v1/conversations/" + conversation_id(context)
}
