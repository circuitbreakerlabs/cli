fn build_request(messages, context) {
    #{
        session_id: context.first_request_id,
        messages: messages,
        previous_replies: context.assistant_replies,
    }
}

fn parse_response(response) {
    if response.status == 429 {
        return #{type: "error", reason: "rate_limited", message: "Try again"};
    }
    if response.status != 200 {
        return #{type: "error", reason: "unknown", message: "Test request failed"};
    }
    #{type: "success", content: response.body.reply, finish_reason: response.body.finish_reason}
}

fn build_close_path(context) {
    "/test/sessions/" + context.first_request_id
}
