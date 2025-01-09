use std::time::Duration;

use anyhow::{anyhow, Context};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use slog_scope::info;

use mithril_common::{
    entities::{Epoch, TransactionHash},
    messages::{
        CardanoDatabaseDigestListMessage, CardanoDatabaseSnapshotListMessage,
        CardanoDatabaseSnapshotMessage, CardanoStakeDistributionListMessage,
        CardanoStakeDistributionMessage, CardanoTransactionSnapshotListMessage,
        CardanoTransactionSnapshotMessage, CertificateMessage, MithrilStakeDistributionListMessage,
        MithrilStakeDistributionMessage, SnapshotMessage,
    },
    StdResult,
};

use crate::{
    attempt, utils::AttemptResult, CardanoDbCommand, CardanoStakeDistributionCommand,
    CardanoTransactionCommand, Client, ClientCommand, MithrilStakeDistributionCommand,
};

async fn get_json_response<T: DeserializeOwned>(url: String) -> StdResult<reqwest::Result<T>> {
    match reqwest::get(url.clone()).await {
        Ok(response) => {
            let r = response.status();
            match r {
                StatusCode::OK => Ok(response.json::<T>().await),
                s => Err(anyhow!("Unexpected status code from Aggregator: {s}")),
            }
        }
        Err(err) => Err(anyhow!(err).context(format!("Request to `{url}` failed"))),
    }
}

pub async fn assert_node_producing_mithril_stake_distribution(
    aggregator_endpoint: &str,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/mithril-stake-distributions");
    info!("Waiting for the aggregator to produce a mithril stake distribution");

    async fn fetch_last_mithril_stake_distribution_hash(url: String) -> StdResult<Option<String>> {
        match get_json_response::<MithrilStakeDistributionListMessage>(url)
            .await?
            .as_deref()
        {
            Ok([stake_distribution, ..]) => Ok(Some(stake_distribution.hash.clone())),
            Ok(&[]) => Ok(None),
            Err(err) => Err(anyhow!("Invalid mithril stake distribution body : {err}",)),
        }
    }

    // todo: reduce the number of attempts if we can reduce the delay between two immutables
    match attempt!(45, Duration::from_millis(2000), {
        fetch_last_mithril_stake_distribution_hash(url.clone()).await
    }) {
        AttemptResult::Ok(hash) => {
            info!("Aggregator produced a mithril stake distribution"; "hash" => &hash);
            Ok(hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_node_producing_mithril_stake_distribution, no response from `{url}`"
        )),
    }
}

pub async fn assert_signer_is_signing_mithril_stake_distribution(
    aggregator_endpoint: &str,
    hash: &str,
    expected_epoch_min: Epoch,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/mithril-stake-distribution/{hash}");
    info!(
        "Asserting the aggregator is signing the mithril stake distribution message `{}` with an expected min epoch of `{}`",
        hash,
        expected_epoch_min
    );

    async fn fetch_mithril_stake_distribution_message(
        url: String,
        expected_epoch_min: Epoch,
    ) -> StdResult<Option<MithrilStakeDistributionMessage>> {
        match get_json_response::<MithrilStakeDistributionMessage>(url)
            .await?
            {
                Ok(stake_distribution) => match stake_distribution.epoch {
                    epoch if epoch >= expected_epoch_min => Ok(Some(stake_distribution)),
                    epoch => Err(anyhow!(
                        "Minimum expected mithril stake distribution epoch not reached : {epoch} < {expected_epoch_min}"
                    )),
                },
                Err(err) => Err(anyhow!("Invalid mithril stake distribution body : {err}",)),
            }
    }

    match attempt!(10, Duration::from_millis(1000), {
        fetch_mithril_stake_distribution_message(url.clone(), expected_epoch_min).await
    }) {
        AttemptResult::Ok(stake_distribution) => {
            // todo: assert that the mithril stake distribution is really signed
            info!("Signer signed a mithril stake distribution"; "certificate_hash" => &stake_distribution.certificate_hash);
            Ok(stake_distribution.certificate_hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_signer_is_signing_mithril_stake_distribution, no response from `{url}`"
        )),
    }
}

pub async fn assert_node_producing_snapshot(aggregator_endpoint: &str) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/snapshots");
    info!("Waiting for the aggregator to produce a snapshot");

    async fn fetch_last_snapshot_digest(url: String) -> StdResult<Option<String>> {
        match get_json_response::<Vec<SnapshotMessage>>(url)
            .await?
            .as_deref()
        {
            Ok([snapshot, ..]) => Ok(Some(snapshot.digest.clone())),
            Ok(&[]) => Ok(None),
            Err(err) => Err(anyhow!("Invalid snapshot body : {err}",)),
        }
    }

    // todo: reduce the number of attempts if we can reduce the delay between two immutables
    match attempt!(45, Duration::from_millis(2000), {
        fetch_last_snapshot_digest(url.clone()).await
    }) {
        AttemptResult::Ok(digest) => {
            info!("Aggregator produced a snapshot"; "digest" => &digest);
            Ok(digest)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_node_producing_snapshot, no response from `{url}`"
        )),
    }
}

pub async fn assert_signer_is_signing_snapshot(
    aggregator_endpoint: &str,
    digest: &str,
    expected_epoch_min: Epoch,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/snapshot/{digest}");
    info!(
        "Asserting the aggregator is signing the snapshot message `{}` with an expected min epoch of `{}`",
        digest,
        expected_epoch_min
    );

    async fn fetch_snapshot_message(
        url: String,
        expected_epoch_min: Epoch,
    ) -> StdResult<Option<SnapshotMessage>> {
        match get_json_response::<SnapshotMessage>(url).await? {
            Ok(snapshot) => match snapshot.beacon.epoch {
                epoch if epoch >= expected_epoch_min => Ok(Some(snapshot)),
                epoch => Err(anyhow!(
                    "Minimum expected snapshot epoch not reached : {epoch} < {expected_epoch_min}"
                )),
            },
            Err(err) => Err(anyhow!(err).context("Invalid snapshot body")),
        }
    }

    match attempt!(10, Duration::from_millis(1000), {
        fetch_snapshot_message(url.clone(), expected_epoch_min).await
    }) {
        AttemptResult::Ok(snapshot) => {
            // todo: assert that the snapshot is really signed
            info!("Signer signed a snapshot"; "certificate_hash" => &snapshot.certificate_hash);
            Ok(snapshot.certificate_hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_signer_is_signing_snapshot, no response from `{url}`"
        )),
    }
}

pub async fn assert_node_producing_cardano_database_snapshot(
    aggregator_endpoint: &str,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-database");
    info!("Waiting for the aggregator to produce a Cardano database snapshot");

    async fn fetch_last_cardano_database_snapshot_hash(url: String) -> StdResult<Option<String>> {
        match get_json_response::<CardanoDatabaseSnapshotListMessage>(url)
            .await?
            .as_deref()
        {
            Ok([cardano_database_snapshot, ..]) => Ok(Some(cardano_database_snapshot.hash.clone())),
            Ok(&[]) => Ok(None),
            Err(err) => Err(anyhow!("Invalid Cardano database snapshot body : {err}",)),
        }
    }

    // todo: reduce the number of attempts if we can reduce the delay between two immutables
    match attempt!(45, Duration::from_millis(2000), {
        fetch_last_cardano_database_snapshot_hash(url.clone()).await
    }) {
        AttemptResult::Ok(hash) => {
            info!("Aggregator produced a Cardano database snapshot"; "hash" => &hash);
            Ok(hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_node_producing_snapshot, no response from `{url}`"
        )),
    }
}

pub async fn assert_signer_is_signing_cardano_database_snapshot(
    aggregator_endpoint: &str,
    hash: &str,
    expected_epoch_min: Epoch,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-database/{hash}");
    info!(
        "Asserting the aggregator is signing the Cardano database snapshot message `{}` with an expected min epoch of `{}`",
        hash,
        expected_epoch_min
    );

    async fn fetch_cardano_database_snapshot_message(
        url: String,
        expected_epoch_min: Epoch,
    ) -> StdResult<Option<CardanoDatabaseSnapshotMessage>> {
        match get_json_response::<CardanoDatabaseSnapshotMessage>(url)
            .await?
            {
                Ok(cardano_database_snapshot) => match cardano_database_snapshot.beacon.epoch {
                    epoch if epoch >= expected_epoch_min => Ok(Some(cardano_database_snapshot)),
                    epoch => Err(anyhow!(
                        "Minimum expected Cardano database snapshot epoch not reached : {epoch} < {expected_epoch_min}"
                    )),
                },
                Err(err) => Err(anyhow!(err).context("Invalid Cardano database snapshot body")),
            }
    }

    match attempt!(10, Duration::from_millis(1000), {
        fetch_cardano_database_snapshot_message(url.clone(), expected_epoch_min).await
    }) {
        AttemptResult::Ok(snapshot) => {
            info!("Signer signed a snapshot"; "certificate_hash" => &snapshot.certificate_hash);
            Ok(snapshot.certificate_hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_signer_is_signing_snapshot, no response from `{url}`"
        )),
    }
}

pub async fn assert_node_producing_cardano_database_digests_map(
    aggregator_endpoint: &str,
) -> StdResult<Vec<(String, String)>> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-database/digests");
    info!("Waiting for the aggregator to produce a Cardano database digests map");

    async fn fetch_cardano_database_digests_map(
        url: String,
    ) -> StdResult<Option<Vec<(String, String)>>> {
        match get_json_response::<CardanoDatabaseDigestListMessage>(url)
            .await?
            .as_deref()
        {
            Ok(&[]) => Ok(None),
            Ok(cardano_database_digests_map) => Ok(Some(
                cardano_database_digests_map
                    .iter()
                    .map(|item| (item.immutable_file_name.clone(), item.digest.clone()))
                    .collect(),
            )),
            Err(err) => Err(anyhow!("Invalid Cardano database digests map body : {err}",)),
        }
    }

    // todo: reduce the number of attempts if we can reduce the delay between two immutables
    match attempt!(45, Duration::from_millis(2000), {
        fetch_cardano_database_digests_map(url.clone()).await
    }) {
        AttemptResult::Ok(cardano_database_digests_map) => {
            info!("Aggregator produced a Cardano database digests map"; "total_digests" => &cardano_database_digests_map.len());
            Ok(cardano_database_digests_map)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_node_producing_cardano_database_digests_map, no response from `{url}`"
        )),
    }
}

pub async fn assert_node_producing_cardano_transactions(
    aggregator_endpoint: &str,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-transactions");
    info!("Waiting for the aggregator to produce a Cardano transactions artifact");

    async fn fetch_last_cardano_transaction_snapshot_hash(
        url: String,
    ) -> StdResult<Option<String>> {
        match get_json_response::<CardanoTransactionSnapshotListMessage>(url)
            .await?
            .as_deref()
        {
            Ok([artifact, ..]) => Ok(Some(artifact.hash.clone())),
            Ok(&[]) => Ok(None),
            Err(err) => Err(anyhow!(
                "Invalid Cardano transactions artifact body : {err}",
            )),
        }
    }

    match attempt!(45, Duration::from_millis(2000), {
        fetch_last_cardano_transaction_snapshot_hash(url.clone()).await
    }) {
        AttemptResult::Ok(hash) => {
            info!("Aggregator produced a Cardano transactions artifact"; "hash" => &hash);
            Ok(hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_node_producing_cardano_transactions, no response from `{url}`"
        )),
    }
}

pub async fn assert_signer_is_signing_cardano_transactions(
    aggregator_endpoint: &str,
    hash: &str,
    expected_epoch_min: Epoch,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-transaction/{hash}");
    info!(
        "Asserting the aggregator is signing the Cardano transactions artifact `{}` with an expected min epoch of `{}`",
        hash,
        expected_epoch_min
    );

    async fn fetch_cardano_transaction_snapshot_message(
        url: String,
        expected_epoch_min: Epoch,
    ) -> StdResult<Option<CardanoTransactionSnapshotMessage>> {
        match get_json_response::<CardanoTransactionSnapshotMessage>(url).await? {
            Ok(artifact) => match artifact.epoch {
                epoch if epoch >= expected_epoch_min => Ok(Some(artifact)),
                epoch => Err(anyhow!(
                    "Minimum expected artifact epoch not reached : {epoch} < {expected_epoch_min}"
                )),
            },
            Err(err) => Err(anyhow!(err).context("Invalid Cardano transactions artifact body")),
        }
    }

    match attempt!(10, Duration::from_millis(1000), {
        fetch_cardano_transaction_snapshot_message(url.clone(), expected_epoch_min).await
    }) {
        AttemptResult::Ok(artifact) => {
            info!("Signer signed a Cardano transactions artifact"; "certificate_hash" => &artifact.certificate_hash);
            Ok(artifact.certificate_hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_signer_is_signing_cardano_transactions, no response from `{url}`"
        )),
    }
}

pub async fn assert_node_producing_cardano_stake_distribution(
    aggregator_endpoint: &str,
) -> StdResult<(String, Epoch)> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-stake-distributions");
    info!("Waiting for the aggregator to produce a Cardano stake distribution");

    async fn fetch_last_cardano_stake_distribution_message(
        url: String,
    ) -> StdResult<Option<(String, Epoch)>> {
        match get_json_response::<CardanoStakeDistributionListMessage>(url)
            .await?
            .as_deref()
        {
            Ok([stake_distribution, ..]) => Ok(Some((
                stake_distribution.hash.clone(),
                stake_distribution.epoch,
            ))),
            Ok(&[]) => Ok(None),
            Err(err) => Err(anyhow!("Invalid Cardano stake distribution body : {err}",)),
        }
    }

    match attempt!(45, Duration::from_millis(2000), {
        fetch_last_cardano_stake_distribution_message(url.clone()).await
    }) {
        AttemptResult::Ok((hash, epoch)) => {
            info!("Aggregator produced a Cardano stake distribution"; "hash" => &hash, "epoch" => #?epoch);
            Ok((hash, epoch))
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_node_producing_cardano_stake_distribution, no response from `{url}`"
        )),
    }
}

pub async fn assert_signer_is_signing_cardano_stake_distribution(
    aggregator_endpoint: &str,
    hash: &str,
    expected_epoch_min: Epoch,
) -> StdResult<String> {
    let url = format!("{aggregator_endpoint}/artifact/cardano-stake-distribution/{hash}");
    info!(
        "Asserting the aggregator is signing the Cardano stake distribution message `{}` with an expected min epoch of `{}`",
        hash,
        expected_epoch_min
    );

    async fn fetch_cardano_stake_distribution_message(
        url: String,
        expected_epoch_min: Epoch,
    ) -> StdResult<Option<CardanoStakeDistributionMessage>> {
        match get_json_response::<CardanoStakeDistributionMessage>(url)
        .await?
        {
            Ok(stake_distribution) => match stake_distribution.epoch {
                epoch if epoch >= expected_epoch_min => Ok(Some(stake_distribution)),
                epoch => Err(anyhow!(
                    "Minimum expected Cardano stake distribution epoch not reached : {epoch} < {expected_epoch_min}"
                )),
            },
            Err(err) => Err(anyhow!(err).context("Invalid Cardano stake distribution body",)),
        }
    }

    match attempt!(10, Duration::from_millis(1000), {
        fetch_cardano_stake_distribution_message(url.clone(), expected_epoch_min).await
    }) {
        AttemptResult::Ok(cardano_stake_distribution) => {
            info!("Signer signed a Cardano stake distribution"; "certificate_hash" => &cardano_stake_distribution.certificate_hash);
            Ok(cardano_stake_distribution.certificate_hash)
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_signer_is_signing_cardano_stake_distribution, no response from `{url}`"
        )),
    }
}

pub async fn assert_is_creating_certificate_with_enough_signers(
    aggregator_endpoint: &str,
    certificate_hash: &str,
    total_signers_expected: usize,
) -> StdResult<()> {
    let url = format!("{aggregator_endpoint}/certificate/{certificate_hash}");

    async fn fetch_certificate_message(url: String) -> StdResult<Option<CertificateMessage>> {
        match get_json_response::<CertificateMessage>(url).await? {
            Ok(certificate) => Ok(Some(certificate)),
            Err(err) => Err(anyhow!(err).context("Invalid snapshot body")),
        }
    }

    match attempt!(10, Duration::from_millis(1000), {
        fetch_certificate_message(url.clone()).await
    }) {
        AttemptResult::Ok(certificate) => {
            info!("Aggregator produced a certificate"; "certificate" => ?certificate);
            if certificate.metadata.signers.len() == total_signers_expected {
                info!(
                    "Certificate is signed by expected number of signers: {} >= {} ",
                    certificate.metadata.signers.len(),
                    total_signers_expected
                );
                Ok(())
            } else {
                Err(anyhow!(
                    "Certificate is not signed by expected number of signers: {} < {} ",
                    certificate.metadata.signers.len(),
                    total_signers_expected
                ))
            }
        }
        AttemptResult::Err(error) => Err(error),
        AttemptResult::Timeout() => Err(anyhow!(
            "Timeout exhausted assert_is_creating_certificate, no response from `{url}`"
        )),
    }
}

pub async fn assert_client_can_verify_snapshot(client: &mut Client, digest: &str) -> StdResult<()> {
    client
        .run(ClientCommand::CardanoDb(CardanoDbCommand::Download {
            digest: digest.to_string(),
        }))
        .await?;
    info!("Client downloaded & restored the snapshot"; "digest" => &digest);

    Ok(())
}

pub async fn assert_client_can_verify_mithril_stake_distribution(
    client: &mut Client,
    hash: &str,
) -> StdResult<()> {
    client
        .run(ClientCommand::MithrilStakeDistribution(
            MithrilStakeDistributionCommand::Download {
                hash: hash.to_owned(),
            },
        ))
        .await?;
    info!("Client downloaded the Mithril stake distribution"; "hash" => &hash);

    Ok(())
}

pub async fn assert_client_can_verify_transactions(
    client: &mut Client,
    tx_hashes: Vec<TransactionHash>,
) -> StdResult<()> {
    #[allow(dead_code)]
    #[derive(Debug, serde::Deserialize)]
    struct ClientCtxCertifyResult {
        certified_transactions: Vec<TransactionHash>,
        non_certified_transactions: Vec<TransactionHash>,
    }

    let result_file = client
        .run(ClientCommand::CardanoTransaction(
            CardanoTransactionCommand::Certify {
                tx_hashes: tx_hashes.clone(),
            },
        ))
        .await?;
    info!("Client verified the Cardano transactions"; "tx_hashes" => ?tx_hashes);

    let file = std::fs::read_to_string(&result_file).with_context(|| {
        format!(
            "Failed to read client output from file `{}`",
            result_file.display()
        )
    })?;
    let result: ClientCtxCertifyResult = serde_json::from_str(&file).with_context(|| {
        format!(
            "Failed to parse client output as json from file `{}`",
            result_file.display()
        )
    })?;

    info!("Asserting that all Cardano transactions where verified by the Client...");
    if tx_hashes
        .iter()
        .all(|tx| result.certified_transactions.contains(tx))
    {
        Ok(())
    } else {
        Err(anyhow!(
            "Not all transactions where certified:\n'{:#?}'",
            result,
        ))
    }
}

pub async fn assert_client_can_verify_cardano_stake_distribution(
    client: &mut Client,
    hash: &str,
    epoch: Epoch,
) -> StdResult<()> {
    client
        .run(ClientCommand::CardanoStakeDistribution(
            CardanoStakeDistributionCommand::Download {
                unique_identifier: epoch.to_string(),
            },
        ))
        .await?;
    info!("Client downloaded the Cardano stake distribution by epoch"; "epoch" => epoch.to_string());

    client
        .run(ClientCommand::CardanoStakeDistribution(
            CardanoStakeDistributionCommand::Download {
                unique_identifier: hash.to_string(),
            },
        ))
        .await?;
    info!("Client downloaded the Cardano stake distribution by hash"; "hash" => hash.to_string());

    Ok(())
}
