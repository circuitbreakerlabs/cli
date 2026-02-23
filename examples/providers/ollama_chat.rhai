// Example Ollama provider script
// This script implements the Ollama chat API spec

// Build the request body that will be POST'd to the endpoint
// messages: array of #{role: String, content: String}
// returns: a map that will be serialized to JSON
fn build_request(messages) {
    #{
        "model": "llama3.2",
        "messages": messages,
        "stream": false
    }
}

// Parse the response body and extract the assistant's message
// body: the full deserialized JSON response as a Rhai dynamic
// returns: String containing the assistant's message content
fn parse_response(body) {
    body["message"]["content"].to_string()
}