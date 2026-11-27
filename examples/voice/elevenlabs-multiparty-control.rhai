// Multiparty voice protocol v2 control hooks. Each exchange contains an ordered
// logical-speaker manifest but is played through one LiveKit participant/track.
fn on_session_event(event, state) {
    let actions = [];
    if event.type == "connected" {
        state.ready = false;
        state.active_exchange = false;
        state.caller_playing = false;
        state.awaiting_recovery = false;
        state.playback_ticks = event.contains("playback_ticks") && event.playback_ticks;
        state.participants = event.participants;
        actions.push(#{type: "send", message: #{type: "conversation_initiation_client_data"}});
    }
    if event.type == "exchange_start" {
        state.active_exchange = true;
        state.exchange_id = event.exchange_id;
        state.inputs = event.inputs;
        state.caller_playing = true;
        state.awaiting_recovery = false;
        if state.playback_ticks {
            actions.push(#{type: "send", message: #{type: "user_activity"}});
        }
        actions.push(#{type: "capture_start"});
        actions.push(#{type: "exchange_ready"});
    }
    if event.type == "playback_tick" && state.playback_ticks && state.caller_playing {
        actions.push(#{type: "send", message: #{type: "user_activity"}});
    }
    if event.type == "playback_complete" && event.exchange_id == state.exchange_id {
        state.caller_playing = false;
    }
    if event.type == "closing" {
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
    if message.type == "agent_response" && message.contains("agent_response_event") && message.agent_response_event.contains("agent_response") && state.active_exchange && event.caller_started {
        state.awaiting_recovery = false;
        actions.push(#{type: "transcript", text: message.agent_response_event.agent_response});
    }
    if message.type == "agent_response_complete" && state.active_exchange && state.caller_playing && event.caller_started {
        state.awaiting_recovery = true;
    }
    if message.type == "agent_response_complete" && state.active_exchange && !state.awaiting_recovery && !state.caller_playing && event.caller_started {
        state.active_exchange = false;
        actions.push(#{type: "response_end"});
    }
    if message.type == "interruption" && state.active_exchange && event.caller_started {
        state.awaiting_recovery = true;
    }
    if message.type == "error" { actions.push(#{type: "error"}); }
    #{state: state, actions: actions}
}
