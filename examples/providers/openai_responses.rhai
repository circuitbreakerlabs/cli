// Example OpenAI-compatible provider script
// This script implements the OpenAI responses API spec

// Build the request body that will be POST'd to the endpoint
// messages: array of #{role: String, content: String}
// returns: a map that will be serialized to JSON
fn build_request(messages) {
    #{
        "model": "gpt-4o",
        "input": messages
    }
}

// Parse the response body and extract the assistant's message
// body: the full deserialized JSON response as a Rhai dynamic
// returns: String containing the assistant's message content
fn parse_response(body) {
    body["output"][0]["content"][0]["text"].to_string()
}
