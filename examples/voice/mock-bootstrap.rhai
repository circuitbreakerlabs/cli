fn build_connect_request(context) {
    let call_id = "cbl-" + context.session_nonce;
    #{ method: "POST", url: context.parameters.bootstrap_url + "/livekit/connect",
       body: #{ call_id: call_id, room_name: call_id, caller_identity: call_id + "-caller", max_turns: context.max_turns } }
}
fn parse_connect_response(body, context) {
    #{ url: body.url, token: body.caller_token }
}
