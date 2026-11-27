fn on_session_event(event, state) {
    let actions = [];
    if event.type == "closing" {
        actions.push(#{type:"send", topic:"voice-control", message:#{type:"call.hangup", reason:"evaluation ended"}});
    }
    if event.type == "connected" {
        actions.push(#{type:"send", topic:"voice-control", message:#{type:"session.configure", call_id:"cbl-" + event.session_nonce, max_turns:event.max_exchanges, participants:event.participants}});
    }
    if event.type == "exchange_start" {
        state.exchange = event.exchange_id;
        state.inputs = event.inputs;
        actions.push(#{type:"capture_stop"});
        actions.push(#{type:"send", topic:"voice-control", message:#{type:"exchange.start", exchange_id:state.exchange, inputs:state.inputs}});
    }
    if event.type == "exchange_end" {
        actions.push(#{type:"send", topic:"voice-control", message:#{type:"exchange.end", exchange_id:state.exchange}});
    }
    #{state:state, actions:actions}
}

fn on_target_event(event, state) {
    let message = event.message;
    let actions = [];
    if message.type == "call.hangup.ack" { actions.push(#{type:"close_ready"}); }
    if message.type == "call.error" { actions.push(#{type:"error"}); }
    if message.type == "session.ready" { actions.push(#{type:"session_ready"}); }
    if message.type == "exchange.ready" && message.exchange_id == state.exchange { actions.push(#{type:"exchange_ready"}); }
    if message.type == "response.start" && message.exchange_id == state.exchange {
        actions.push(#{type:"capture_start"});
    }
    if message.type == "response.end" && message.exchange_id == state.exchange {
        if message.contains("transcript") { actions.push(#{type:"transcript", text:message.transcript}); }
        actions.push(#{type:"response_end"});
    }
    #{state:state, actions:actions}
}
