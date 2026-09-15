// Enable agent_response_complete in the agent's client_events configuration.
// Recovery additionally requires agent_response transcript events.
// Use a caller-first agent; unsolicited greetings are not evaluated here.
fn on_session_event(event, state) {
    let actions = [];
    if event.type == "connected" {
        state.ready = false;
        state.active_turn = false;
        state.caller_playing = false;
        state.awaiting_recovery = false;
        state.playback_ticks = event.contains("playback_ticks") && event.playback_ticks;
        actions.push(#{type: "send", message: #{type: "conversation_initiation_client_data"}});
    }
    if event.type == "utterance_start" {
        state.active_turn = true;
        state.caller_playing = true;
        state.awaiting_recovery = false;
        if event.contains("stream_id") {
            state.stream_id = event.stream_id;
        }
        // Send before releasing the audio gate, then refresh while publishing.
        if state.playback_ticks {
            actions.push(#{type: "send", message: #{type: "user_activity"}});
        }
        actions.push(#{type: "capture_start"});
        actions.push(#{type: "utterance_ready"});
    }
    if event.type == "playback_tick" && state.playback_ticks && state.caller_playing && state.active_turn && event.stream_id == state.stream_id {
        actions.push(#{type: "send", message: #{type: "user_activity"}});
    }
    if event.type == "closing" {
        state.caller_playing = false;
    }
    if event.type == "playback_complete" && (!event.contains("stream_id") || !state.contains("stream_id") || event.stream_id == state.stream_id) {
        state.caller_playing = false;
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
    if message.type == "agent_response" && message.contains("agent_response_event") && message.agent_response_event.contains("agent_response") && state.active_turn && (!event.contains("caller_started") || event.caller_started) {
        state.awaiting_recovery = false;
        actions.push(#{type: "transcript", text: message.agent_response_event.agent_response});
    }
    // A completion received while caller audio is still playing is stale for
    // this boundary. Require fresh response text before accepting a later
    // completion. Fresh text is a recovery heuristic, not generation correlation.
    if message.type == "agent_response_complete" && state.active_turn && state.caller_playing && (!event.contains("caller_started") || event.caller_started) {
        state.awaiting_recovery = true;
    }
    if message.type == "agent_response_complete" && state.active_turn && !state.awaiting_recovery && !state.caller_playing && (!event.contains("caller_started") || event.caller_started) {
        state.active_turn = false;
        actions.push(#{type: "response_end"});
    }
    // An interruption cancels the provider response, but it is not a complete
    // response for this evaluation. Keep capture open so a later uninterrupted
    // response can complete the turn. If the provider never recovers, the
    // existing API/session timeout remains the bounded fallback.
    if message.type == "interruption" && state.active_turn && (!event.contains("caller_started") || event.caller_started) {
        state.awaiting_recovery = true;
    }
    if message.type == "error" {
        actions.push(#{type: "error"});
    }
    #{state: state, actions: actions}
}
