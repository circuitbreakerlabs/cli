// Enable agent_response_complete in the agent's client_events configuration.
// Use a caller-first agent; unsolicited greetings are not evaluated here.
fn on_session_event(event, state) {
    let actions = [];
    if event.type == "connected" {
        state.ready = false;
        state.active_turn = false;
        actions.push(#{type: "send", message: #{type: "conversation_initiation_client_data"}});
    }
    if event.type == "utterance_start" {
        state.active_turn = true;
        actions.push(#{type: "capture_start"});
        actions.push(#{type: "utterance_ready"});
    }
    #{state: state, actions: actions}
}

fn on_target_event(event, state) {
    let message = event.message;
    let actions = [];
    if !message.contains("type") { return #{state: state, actions: actions}; }
    if message.type == "conversation_initiation_metadata" && !state.ready {
        state.ready = true;
        actions.push(#{type: "session_ready"});
    }
    if message.type == "ping" {
        actions.push(#{type: "send", message: #{type: "pong", event_id: message.ping_event.event_id}});
    }
    if message.type == "agent_response" && state.active_turn {
        actions.push(#{type: "transcript", text: message.agent_response_event.agent_response});
    }
    if message.type == "agent_response_complete" && state.active_turn {
        state.active_turn = false;
        actions.push(#{type: "response_end"});
    }
    if message.type == "error" {
        actions.push(#{type: "error"});
    }
    #{state: state, actions: actions}
}
