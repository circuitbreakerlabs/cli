mod connect;

pub use connect::{WebSocketConnection, connect};

pub struct WebSocketClose {
    pub code: u16,
    pub reason: String,
}
