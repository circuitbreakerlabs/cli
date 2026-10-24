// Example OpenAI-compatible provider script
// This script implements the OpenAI chat completions API spec

// Build the request body that will be POST'd to the endpoint
// messages: array of #{role: String, content: String}
// returns: a map that will be serialized to JSON
fn build_request(messages) {
    #{
        "model": "gpt-4o",
        "messages": messages
    }
}

// response: #{status, headers, body, body_text, body_parse_error, url, elapsed_ms}
// Header names are lowercase; values are arrays, e.g. response.headers["retry-after"][0].
// Return a structured success or an error using a documented completion error reason.
// Rate-limit errors may include retry_after_seconds (a nonnegative integer).
// It overrides HTTP Retry-After; omit it to use the header or default schedule.
// The retry loop still waits at least 2, 4, then 8 seconds for successive retries.
fn parse_response(response) {
    let reason = "unknown";
    if response.status == 401 { reason = "authentication_failed"; }
    else if response.status == 429 { reason = "rate_limited"; }
    else if response.status == 408 || response.status == 504 { reason = "model_timeout"; }

    let body = response.body;
    let error_message = "";
    if type_of(body) == "map" && body.contains("error") && body.error != () {
        if type_of(body.error) == "map" && body.error.contains("message") {
            error_message = body.error.message.to_string();
        } else {
            error_message = body.error.to_string();
        }
        if error_message == "" { error_message = "Provider returned an error"; }
    }
    if response.status < 200 || response.status >= 300 || error_message != "" {
        if error_message == "" { error_message = "HTTP " + response.status.to_string(); }
        let error = #{type: "error", reason: reason, message: error_message};
        // Adapt this optional body field to your endpoint, or set a custom delay:
        // error.retry_after_seconds = 30;
        if reason == "rate_limited" && type_of(body) == "map"
            && body.contains("retry_after_seconds")
            && type_of(body.retry_after_seconds) == "i64" && body.retry_after_seconds >= 0 {
            error.retry_after_seconds = body.retry_after_seconds;
        }
        return error;
    }
    if response.body_parse_error != () {
        return #{type: "error", reason: "invalid_response", message: response.body_parse_error};
    }
    try {
        let content = body["choices"][0]["message"]["content"];
        if type_of(content) == "string" && content != "" {
            return #{type: "success", content: content};
        }
    } catch (error) {
        // A missing or malformed provider field is an invalid response.
    }
    #{type: "error", reason: "invalid_response", message: "Missing or invalid assistant content"}
}
