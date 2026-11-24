fn conversation_id(context) {
    "cbl-" + context.first_request_id
}

fn build_request(messages, context) {
    let history = [];
    let names = [];
    let utterances = [];
    let assistant_index = 0;
    let raw_replies = context.assistant_replies;

    for message in messages {
        if message.role == "system" {
            continue;
        }
        if message.role == "assistant" {
            if utterances.len != 2 || assistant_index >= raw_replies.len {
                throw "Invalid CoGo dual conversation history";
            }
            let raw_reply = raw_replies[assistant_index];
            history.push(#{role: "assistant", content: raw_reply});
            assistant_index += 1;
            utterances = [];
            continue;
        }

        let speaker = utterances.len;
        if speaker >= 2 || !message.contains("name") {
            throw "CoGo dual requires two named participants per round";
        }
        let name = message.name;
        if name == "" {
            throw "CoGo dual requires two named participants per round";
        }
        if names.len < 2 {
            if names.contains(name) {
                throw "CoGo dual requires distinct participant names";
            }
            names.push(name);
        } else if name != names[speaker] {
            throw "CoGo dual requires the same participant order each round";
        }

        let utterance = name + ": " + message.content;
        utterances.push(utterance);
        if utterances.len < 2 {
            continue;
        }
        if assistant_index >= 30 {
            throw "CoGo allows at most 30 user messages per conversation";
        }
        let content = utterances[0] + "\n" + utterances[1];
        if content.len > 8000 {
            throw "Combined CoGo user message exceeds 8000 characters";
        }
        history.push(#{role: "user", content: content});
    }
    if utterances.len != 2 || assistant_index != raw_replies.len {
        throw "Invalid CoGo dual conversation history";
    }

    let metadata = #{conversation_id: conversation_id(context)};
    if assistant_index == 0 {
        metadata.person1_name = names[0];
        metadata.person2_name = names[1];
    }
    #{
        model: "cogo-dual",
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
