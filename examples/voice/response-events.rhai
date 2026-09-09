// Example JSON control convention. Adapt the event names to your endpoint.
fn on_session_event(event, state) {
    let actions = [];
    if event.type == "connected" { actions.push(#{type: "capture_start"}); actions.push(#{type: "session_ready"}); }
    if event.type == "utterance_start" {
        actions.push(#{type: "capture_start"});
        actions.push(#{type: "utterance_ready"});
    }
    #{state: state, actions: actions}
}
fn on_target_event(event, state) {
    let message = event.message;
    let actions = [];
    if message.contains("type") {
        if message.type == "agent_response_complete" { actions.push(#{type: "response_end"}); }
        if message.type == "agent_response" && message.contains("agent_response_event") {
            actions.push(#{type: "transcript", text: message.agent_response_event.agent_response});
        }
    }
    #{state: state, actions: actions}
}
