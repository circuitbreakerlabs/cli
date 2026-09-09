fn build_connect_request(context) {
    let url = "https://api.elevenlabs.io/v1/convai/conversation/token?agent_id=" + url_encode(context.parameters.agent_id);
    if context.parameters.contains("branch_id") {
        url += "&branch_id=" + url_encode(context.parameters.branch_id);
    }
    if context.parameters.contains("environment") {
        url += "&environment=" + url_encode(context.parameters.environment);
    }
    #{ method: "GET", url: url, headers: #{ "xi-api-key": context.credentials.api_key } }
}
fn parse_connect_response(body, context) {
    let connection = #{ url: context.parameters.livekit_url, token: body.token };
    if body.contains("conversation_id") { connection.external_session_id = body.conversation_id; }
    connection
}
