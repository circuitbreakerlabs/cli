fn on_session_event(event, state) {
    let actions = [];
    if event.type == "closing" {
        actions.push(#{type:"send", topic:"voice-control", message:#{type:"call.hangup", reason:"evaluation ended"}});
    }
    if event.type == "connected" {
        actions.push(#{type: "send", topic:"voice-control", message: #{type:"session.configure", call_id: "cbl-" + event.session_nonce, max_turns:event.max_turns}});
    }
    if event.type == "utterance_start" {
        state.turn = event.stream_id;
        actions.push(#{type: "capture_stop"});
        actions.push(#{type: "send", topic:"voice-control", message: #{type:"utterance.start", turn_index:state.turn}});
    }
    if event.type == "playback_complete" {
        actions.push(#{type: "send", topic:"voice-control", message: #{type:"utterance.end", turn_index:state.turn, text:event.text}});
    }
    #{state:state, actions:actions}
}
fn on_target_event(event, state) {
    let message = event.message;
    let actions = [];
    if message.type == "call.hangup.ack" { actions.push(#{type:"close_ready"}); }
    if message.type == "call.error" { actions.push(#{type:"error"}); }
    if message.type == "session.ready" { actions.push(#{type:"session_ready"}); }
    if message.type == "utterance.ready" && message.turn_index == state.turn { actions.push(#{type:"utterance_ready"}); }
    if message.type == "response.start" && message.turn_index == state.turn + 1 {
        actions.push(#{type:"capture_start"});
        actions.push(#{type:"send", topic:"voice-control", message:#{type:"response.ready", turn_index:state.turn + 1}});
    }
    if message.type == "response.end" { actions.push(#{type:"response_end"}); }
    #{state:state, actions:actions}
}
