//! Gamma API client for market discovery
//!
//! Fetches active 15-minute BTC up/down markets from Polymarket's Gamma API.
//! These markets resolve based on whether BTC price is above or below the
//! opening price at the end of the 15-minute window.

use super::Market;
use chrono::{DateTime, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::time::Duration;

/// Gamma API base URL
pub const GAMMA_API_URL: &str = "https://gamma-api.polymarket.com";

/// Configuration for the Gamma client
#[derive(Debug, Clone)]
pub struct GammaConfig {
    /// Base URL for the Gamma API
    pub base_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Search term for BTC markets
    pub btc_search_term: String,
    /// Market duration filter (e.g., "15" for 15-minute markets)
    pub duration_filter: Option<String>,
}

impl Default for GammaConfig {
    fn default() -> Self {
        Self {
            base_url: GAMMA_API_URL.to_string(),
            timeout: Duration::from_secs(10),
            btc_search_term: "bitcoin".to_string(),
            duration_filter: Some("15".to_string()),
        }
    }
}

/// Client for Polymarket's Gamma API
pub struct GammaClient {
    config: GammaConfig,
    client: Client,
}

impl GammaClient {
    /// Create a new Gamma API client with default configuration
    pub fn new() -> Self {
        Self::with_config(GammaConfig::default())
    }

    /// Create a new client with custom configuration
    pub fn with_config(config: GammaConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    /// Fetch active 15-minute BTC up/down markets
    ///
    /// Returns markets that are:
    /// - Currently active (not closed or archived)
    /// - Related to BTC/Bitcoin
    /// - 15-minute duration windows
    pub async fn fetch_btc_markets(&self) -> anyhow::Result<Vec<Market>> {
        // Query the series endpoint for BTC 15-minute markets
        let url = format!("{}/series", self.config.base_url);

        tracing::debug!(url = %url, "Fetching BTC 15m series from Gamma API");

        let response = self
            .client
            .get(&url)
            .query(&[("slug", "btc-up-or-down-15m")])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Gamma API error: {} - {}", status, body);
        }

        let series_list: Vec<GammaSeries> = response.json().await?;

        // Get events from the first matching series
        let events: Vec<GammaEvent> = series_list
            .into_iter()
            .flat_map(|s| s.events.unwrap_or_default())
            .filter(|e| e.active && !e.closed)
            .collect();

        tracing::debug!(
            active_events = events.len(),
            "Found active 15-minute BTC events"
        );

        // Convert events to markets (need to fetch market details for each)
        let mut btc_markets = Vec::new();
        for event in events.iter().take(5) {
            // Limit to 5 active markets
            if let Some(market) = self.fetch_market_by_event_slug(&event.slug).await? {
                btc_markets.push(market);
            }
        }

        tracing::info!(
            btc_market_count = btc_markets.len(),
            "Found active 15-minute BTC markets"
        );

        Ok(btc_markets)
    }

    /// Fetch market details by event slug
    async fn fetch_market_by_event_slug(&self, event_slug: &str) -> anyhow::Result<Option<Market>> {
        let url = format!("{}/markets", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("slug", event_slug)]) // Use 'slug' not 'event_slug'
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let gamma_markets: Vec<GammaMarket> = response.json().await?;

        // Get the first market that has token IDs
        for market in gamma_markets {
            if market.clob_token_ids.is_some() {
                return self.convert_to_market(market).map(Some);
            }
        }

        Ok(None)
    }

    /// Fetch a specific market by condition ID
    pub async fn fetch_market(&self, condition_id: &str) -> anyhow::Result<Option<Market>> {
        let url = format!("{}/markets", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("condition_ids", condition_id)])
            .send()
            .await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let gamma_markets: Vec<GammaMarket> = response.json().await?;

        gamma_markets
            .into_iter()
            .next()
            .map(|m| self.convert_to_market(m))
            .transpose()
    }

    /// Check if a market is a 15-minute BTC up/down market
    #[allow(dead_code)]
    fn is_btc_15min_market(&self, market: &GammaMarket) -> bool {
        let question_lower = market.question.to_lowercase();

        // Check for BTC/Bitcoin keywords
        let is_btc = question_lower.contains("btc")
            || question_lower.contains("bitcoin")
            || question_lower.contains("₿");

        // Check for up/down pattern
        let is_up_down = question_lower.contains("up")
            || question_lower.contains("down")
            || question_lower.contains("higher")
            || question_lower.contains("lower")
            || question_lower.contains("above")
            || question_lower.contains("below");

        // Check for 15-minute duration if filter is set
        let is_15min = match &self.config.duration_filter {
            Some(duration) => {
                question_lower.contains(&format!("{} min", duration))
                    || question_lower.contains(&format!("{}-min", duration))
                    || question_lower.contains(&format!("{}min", duration))
                    || question_lower.contains(&format!("{} minute", duration))
            }
            None => true,
        };

        // Must have CLOB token IDs for trading
        let has_tokens = market.clob_token_ids.is_some();

        is_btc && is_up_down && is_15min && has_tokens
    }

    /// Convert a GammaMarket to our Market type
    fn convert_to_market(&self, gamma: GammaMarket) -> anyhow::Result<Market> {
        // Parse CLOB token IDs - format is "[\"token1\", \"token2\"]"
        let token_ids = gamma
            .clob_token_ids
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing clobTokenIds"))?;

        let (yes_token_id, no_token_id) = parse_token_ids(token_ids)?;

        // Parse open price from outcome prices if available
        let open_price = gamma
            .outcome_prices
            .as_ref()
            .and_then(|p| parse_outcome_price(p))
            .unwrap_or_else(|| Decimal::new(5, 1)); // Default to 0.5 if not available

        // Parse timestamps
        let open_time = gamma
            .start_date
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let close_time = gamma
            .end_date
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| open_time + chrono::Duration::minutes(15));

        Ok(Market {
            condition_id: gamma.condition_id,
            yes_token_id,
            no_token_id,
            open_price,
            open_time,
            close_time,
        })
    }
}

impl Default for GammaClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Series response from Gamma API (contains multiple events)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaSeries {
    /// Series slug (e.g., "btc-up-or-down-15m")
    #[allow(dead_code)]
    slug: String,
    /// Series title
    #[allow(dead_code)]
    title: Option<String>,
    /// Events within this series
    events: Option<Vec<GammaEvent>>,
}

/// Event within a series (represents a single 15-minute window)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaEvent {
    /// Event slug (e.g., "btc-updown-15m-1767638700")
    slug: String,
    /// Event title
    #[allow(dead_code)]
    title: Option<String>,
    /// Whether the event is active
    #[serde(default)]
    active: bool,
    /// Whether the event is closed
    #[serde(default)]
    closed: bool,
}

/// Raw market response from Gamma API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaMarket {
    /// Condition ID for the market
    condition_id: String,
    /// Market question
    #[allow(dead_code)]
    question: String,
    /// CLOB token IDs as JSON string
    clob_token_ids: Option<String>,
    /// Outcome prices as JSON string
    outcome_prices: Option<String>,
    /// Market start date
    start_date: Option<String>,
    /// Market end date
    end_date: Option<String>,
    /// Whether market is active (used for filtering in API response)
    #[serde(default)]
    #[allow(dead_code)]
    active: bool,
    /// Whether market is closed (used for filtering in API response)
    #[serde(default)]
    #[allow(dead_code)]
    closed: bool,
}

/// Parse CLOB token IDs from JSON string
///
/// Format: "[\"token1\", \"token2\"]" where token1 is YES and token2 is NO
fn parse_token_ids(token_ids_str: &str) -> anyhow::Result<(String, String)> {
    // Try parsing as JSON array
    let tokens: Vec<String> = serde_json::from_str(token_ids_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse clobTokenIds: {} - {}", token_ids_str, e))?;

    if tokens.len() < 2 {
        anyhow::bail!(
            "Expected 2 token IDs, got {}: {}",
            tokens.len(),
            token_ids_str
        );
    }

    // First token is YES, second is NO
    Ok((tokens[0].clone(), tokens[1].clone()))
}

/// Parse outcome price from JSON string
///
/// Format: "[\"0.52\", \"0.48\"]" - returns the first price (YES price)
fn parse_outcome_price(prices_str: &str) -> Option<Decimal> {
    let prices: Vec<String> = serde_json::from_str(prices_str).ok()?;
    prices.first().and_then(|p| Decimal::from_str(p).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_gamma_client_creation() {
        let client = GammaClient::new();
        assert_eq!(client.config.base_url, GAMMA_API_URL);
    }

    #[test]
    fn test_gamma_config_default() {
        let config = GammaConfig::default();
        assert_eq!(config.base_url, GAMMA_API_URL);
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.btc_search_term, "bitcoin");
    }

    #[test]
    fn test_parse_token_ids() {
        let json = r#"["123456789", "987654321"]"#;
        let (yes, no) = parse_token_ids(json).unwrap();
        assert_eq!(yes, "123456789");
        assert_eq!(no, "987654321");
    }

    #[test]
    fn test_parse_token_ids_invalid() {
        let result = parse_token_ids("invalid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_token_ids_single() {
        let json = r#"["only_one"]"#;
        let result = parse_token_ids(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_outcome_price() {
        let json = r#"["0.52", "0.48"]"#;
        let price = parse_outcome_price(json);
        assert_eq!(price, Some(dec!(0.52)));
    }

    #[test]
    fn test_parse_outcome_price_invalid() {
        let price = parse_outcome_price("not json");
        assert!(price.is_none());
    }

    #[test]
    fn test_is_btc_15min_market() {
        let client = GammaClient::new();

        // Valid 15-min BTC market
        let valid_market = GammaMarket {
            condition_id: "0x123".to_string(),
            question: "Will BTC be up in the next 15 minutes?".to_string(),
            clob_token_ids: Some(r#"["tok1", "tok2"]"#.to_string()),
            outcome_prices: Some(r#"["0.50", "0.50"]"#.to_string()),
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };
        assert!(client.is_btc_15min_market(&valid_market));

        // Bitcoin variant
        let bitcoin_market = GammaMarket {
            condition_id: "0x456".to_string(),
            question: "Bitcoin 15-minute up or down?".to_string(),
            clob_token_ids: Some(r#"["tok1", "tok2"]"#.to_string()),
            outcome_prices: None,
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };
        assert!(client.is_btc_15min_market(&bitcoin_market));

        // Not a BTC market
        let eth_market = GammaMarket {
            condition_id: "0x789".to_string(),
            question: "Will ETH be up in 15 minutes?".to_string(),
            clob_token_ids: Some(r#"["tok1", "tok2"]"#.to_string()),
            outcome_prices: None,
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };
        assert!(!client.is_btc_15min_market(&eth_market));

        // Not 15-min
        let hourly_market = GammaMarket {
            condition_id: "0xabc".to_string(),
            question: "Will BTC be up in 1 hour?".to_string(),
            clob_token_ids: Some(r#"["tok1", "tok2"]"#.to_string()),
            outcome_prices: None,
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };
        assert!(!client.is_btc_15min_market(&hourly_market));

        // No token IDs
        let no_tokens = GammaMarket {
            condition_id: "0xdef".to_string(),
            question: "Will BTC be up in 15 minutes?".to_string(),
            clob_token_ids: None,
            outcome_prices: None,
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };
        assert!(!client.is_btc_15min_market(&no_tokens));
    }

    #[test]
    fn test_convert_to_market() {
        let client = GammaClient::new();

        let gamma = GammaMarket {
            condition_id: "0x123abc".to_string(),
            question: "Will BTC be up in 15 minutes?".to_string(),
            clob_token_ids: Some(r#"["yes_token_123", "no_token_456"]"#.to_string()),
            outcome_prices: Some(r#"["0.55", "0.45"]"#.to_string()),
            start_date: Some("2024-01-15T10:00:00Z".to_string()),
            end_date: Some("2024-01-15T10:15:00Z".to_string()),
            active: true,
            closed: false,
        };

        let market = client.convert_to_market(gamma).unwrap();

        assert_eq!(market.condition_id, "0x123abc");
        assert_eq!(market.yes_token_id, "yes_token_123");
        assert_eq!(market.no_token_id, "no_token_456");
        assert_eq!(market.open_price, dec!(0.55));
    }

    #[test]
    fn test_convert_to_market_missing_tokens() {
        let client = GammaClient::new();

        let gamma = GammaMarket {
            condition_id: "0x123".to_string(),
            question: "Test".to_string(),
            clob_token_ids: None,
            outcome_prices: None,
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };

        let result = client.convert_to_market(gamma);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_to_market_default_price() {
        let client = GammaClient::new();

        let gamma = GammaMarket {
            condition_id: "0x123".to_string(),
            question: "Test".to_string(),
            clob_token_ids: Some(r#"["a", "b"]"#.to_string()),
            outcome_prices: None, // No prices
            start_date: None,
            end_date: None,
            active: true,
            closed: false,
        };

        let market = client.convert_to_market(gamma).unwrap();
        assert_eq!(market.open_price, dec!(0.5)); // Default
    }

    #[test]
    fn test_gamma_config_custom() {
        let config = GammaConfig {
            base_url: "https://test.example.com".to_string(),
            timeout: Duration::from_secs(30),
            btc_search_term: "BTC".to_string(),
            duration_filter: None,
        };

        let client = GammaClient::with_config(config);
        assert_eq!(client.config.base_url, "https://test.example.com");
        assert_eq!(client.config.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_is_btc_market_variations() {
        let client = GammaClient::new();

        // Test various BTC naming conventions
        let variations = vec![
            "Will BTC go up in 15 minutes?",
            "Bitcoin 15-min higher or lower",
            "₿ above opening in 15min window",
            "15 minute BTC down prediction",
        ];

        for question in variations {
            let market = GammaMarket {
                condition_id: "0x1".to_string(),
                question: question.to_string(),
                clob_token_ids: Some(r#"["a", "b"]"#.to_string()),
                outcome_prices: None,
                start_date: None,
                end_date: None,
                active: true,
                closed: false,
            };
            assert!(
                client.is_btc_15min_market(&market),
                "Should match: {}",
                question
            );
        }
    }
}
