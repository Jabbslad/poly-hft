//! WebSocket types and configuration

use std::time::Duration;

/// WebSocket client configuration
#[derive(Debug, Clone)]
pub struct WsConfig {
    /// WebSocket URL to connect to
    pub url: String,
    /// Maximum reconnection attempts before giving up (0 = infinite)
    pub max_reconnect_attempts: u32,
    /// Initial delay before first reconnection attempt
    pub initial_reconnect_delay: Duration,
    /// Maximum delay between reconnection attempts
    pub max_reconnect_delay: Duration,
    /// Interval for sending ping frames
    pub ping_interval: Duration,
    /// Timeout for pong response
    pub pong_timeout: Duration,
}

impl Default for WsConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_reconnect_attempts: 10,
            initial_reconnect_delay: Duration::from_secs(1),
            max_reconnect_delay: Duration::from_secs(60),
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
        }
    }
}

impl WsConfig {
    /// Create a new config with the given URL
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    /// Set maximum reconnection attempts
    pub fn max_reconnects(mut self, n: u32) -> Self {
        self.max_reconnect_attempts = n;
        self
    }

    /// Set initial reconnection delay
    pub fn initial_delay(mut self, d: Duration) -> Self {
        self.initial_reconnect_delay = d;
        self
    }

    /// Set maximum reconnection delay
    pub fn max_delay(mut self, d: Duration) -> Self {
        self.max_reconnect_delay = d;
        self
    }

    /// Set ping interval
    pub fn ping_interval(mut self, d: Duration) -> Self {
        self.ping_interval = d;
        self
    }
}

/// WebSocket message types
#[derive(Debug, Clone)]
pub enum WsMessage {
    /// Text message
    Text(String),
    /// Binary message
    Binary(Vec<u8>),
    /// Connection established
    Connected,
    /// Connection closed
    Disconnected,
    /// Reconnecting after failure
    Reconnecting { attempt: u32 },
}

/// WebSocket errors
#[derive(Debug, Clone)]
pub enum WsError {
    /// Connection failed
    ConnectionFailed(String),
    /// Maximum reconnection attempts exceeded
    MaxReconnectsExceeded,
    /// Channel closed
    ChannelClosed,
    /// Send failed
    SendFailed(String),
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            WsError::MaxReconnectsExceeded => write!(f, "Maximum reconnection attempts exceeded"),
            WsError::ChannelClosed => write!(f, "Channel closed"),
            WsError::SendFailed(e) => write!(f, "Send failed: {}", e),
        }
    }
}

impl std::error::Error for WsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_config_default() {
        let config = WsConfig::default();
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.initial_reconnect_delay, Duration::from_secs(1));
        assert_eq!(config.max_reconnect_delay, Duration::from_secs(60));
        assert_eq!(config.ping_interval, Duration::from_secs(30));
    }

    #[test]
    fn test_ws_config_builder() {
        let config = WsConfig::new("wss://example.com")
            .max_reconnects(5)
            .initial_delay(Duration::from_millis(500))
            .max_delay(Duration::from_secs(30))
            .ping_interval(Duration::from_secs(15));

        assert_eq!(config.url, "wss://example.com");
        assert_eq!(config.max_reconnect_attempts, 5);
        assert_eq!(config.initial_reconnect_delay, Duration::from_millis(500));
        assert_eq!(config.max_reconnect_delay, Duration::from_secs(30));
        assert_eq!(config.ping_interval, Duration::from_secs(15));
    }

    #[test]
    fn test_ws_error_display() {
        let err = WsError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");

        let err = WsError::MaxReconnectsExceeded;
        assert_eq!(err.to_string(), "Maximum reconnection attempts exceeded");
    }

    #[test]
    fn test_ws_message_variants() {
        let msg = WsMessage::Text("hello".to_string());
        assert!(matches!(msg, WsMessage::Text(_)));

        let msg = WsMessage::Connected;
        assert!(matches!(msg, WsMessage::Connected));

        let msg = WsMessage::Reconnecting { attempt: 3 };
        assert!(matches!(msg, WsMessage::Reconnecting { attempt: 3 }));
    }
}
