//! A client to retrieve Cardano stake distributions data from an Aggregator.
//!
//! In order to do so it defines a [CardanoStakeDistributionClient] which exposes the following features:
//!  - [get][CardanoStakeDistributionClient::get]: get a Cardano stake distribution data from its hash
//!
//! # Get a Cardano stake distribution
//!
//! To get a Cardano stake distribution using the [ClientBuilder][crate::client::ClientBuilder].
//!
//! ```no_run
//! # async fn run() -> mithril_client::MithrilResult<()> {
//! use mithril_client::ClientBuilder;
//!
//! let client = ClientBuilder::aggregator("YOUR_AGGREGATOR_ENDPOINT", "YOUR_GENESIS_VERIFICATION_KEY").build()?;
//! let cardano_stake_distribution = client.cardano_stake_distribution().get("CARDANO_STAKE_DISTRIBUTION_HASH").await?.unwrap();
//!
//! println!(
//!     "Cardano stake distribution hash={}, epoch={}, stake_distribution={:?}",
//!     cardano_stake_distribution.hash,
//!     cardano_stake_distribution.epoch,
//!     cardano_stake_distribution.stake_distribution
//! );
//! #    Ok(())
//! # }
//! ```

use anyhow::Context;
use std::sync::Arc;

use crate::aggregator_client::{AggregatorClient, AggregatorClientError, AggregatorRequest};
use crate::{CardanoStakeDistribution, MithrilResult};

/// HTTP client for CardanoStakeDistribution API from the Aggregator
pub struct CardanoStakeDistributionClient {
    aggregator_client: Arc<dyn AggregatorClient>,
}

impl CardanoStakeDistributionClient {
    /// Constructs a new `CardanoStakeDistribution`.
    pub fn new(aggregator_client: Arc<dyn AggregatorClient>) -> Self {
        Self { aggregator_client }
    }

    /// Get the given Cardano stake distribution data. If it cannot be found, a None is returned.
    pub async fn get(&self, hash: &str) -> MithrilResult<Option<CardanoStakeDistribution>> {
        match self
            .aggregator_client
            .get_content(AggregatorRequest::GetCardanoStakeDistribution {
                hash: hash.to_string(),
            })
            .await
        {
            Ok(content) => {
                let cardano_stake_distribution: CardanoStakeDistribution =
                    serde_json::from_str(&content).with_context(|| {
                        "CardanoStakeDistribution client can not deserialize artifact"
                    })?;

                Ok(Some(cardano_stake_distribution))
            }
            Err(AggregatorClientError::RemoteServerLogical(_)) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use chrono::{DateTime, Utc};
    use mockall::predicate::eq;

    use crate::aggregator_client::MockAggregatorHTTPClient;
    use crate::common::{Epoch, StakeDistribution};

    use super::*;

    #[tokio::test]
    async fn get_cardano_stake_distribution_returns_message() {
        let expected_stake_distribution = StakeDistribution::from([("pool123".to_string(), 123)]);
        let message = CardanoStakeDistribution {
            epoch: Epoch(3),
            hash: "hash-123".to_string(),
            certificate_hash: "certificate-hash-123".to_string(),
            stake_distribution: expected_stake_distribution.clone(),
            created_at: DateTime::<Utc>::default(),
        };
        let mut http_client = MockAggregatorHTTPClient::new();
        http_client
            .expect_get_content()
            .with(eq(AggregatorRequest::GetCardanoStakeDistribution {
                hash: "hash-123".to_string(),
            }))
            .return_once(move |_| Ok(serde_json::to_string(&message).unwrap()));
        let client = CardanoStakeDistributionClient::new(Arc::new(http_client));

        let cardano_stake_distribution = client
            .get("hash-123")
            .await
            .unwrap()
            .expect("This test returns a Cardano stake distribution");

        assert_eq!("hash-123".to_string(), cardano_stake_distribution.hash);
        assert_eq!(Epoch(3), cardano_stake_distribution.epoch);
        assert_eq!(
            expected_stake_distribution,
            cardano_stake_distribution.stake_distribution
        );
    }

    #[tokio::test]
    async fn get_cardano_stake_distribution_returns_error_when_invalid_json_structure_in_response()
    {
        let mut http_client = MockAggregatorHTTPClient::new();
        http_client
            .expect_get_content()
            .return_once(move |_| Ok("invalid json structure".to_string()));
        let client = CardanoStakeDistributionClient::new(Arc::new(http_client));

        client
            .get("hash-123")
            .await
            .expect_err("Get Cardano stake distribution should return an error");
    }

    #[tokio::test]
    async fn get_cardano_stake_distribution_returns_none_when_not_found_or_remote_server_logical_error(
    ) {
        let mut http_client = MockAggregatorHTTPClient::new();
        http_client.expect_get_content().return_once(move |_| {
            Err(AggregatorClientError::RemoteServerLogical(anyhow!(
                "not found"
            )))
        });
        let client = CardanoStakeDistributionClient::new(Arc::new(http_client));

        let result = client.get("hash-123").await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_cardano_stake_distribution_returns_error() {
        let mut http_client = MockAggregatorHTTPClient::new();
        http_client
            .expect_get_content()
            .return_once(move |_| Err(AggregatorClientError::SubsystemError(anyhow!("error"))));
        let client = CardanoStakeDistributionClient::new(Arc::new(http_client));

        client
            .get("hash-123")
            .await
            .expect_err("Get Cardano stake distribution should return an error");
    }
}
