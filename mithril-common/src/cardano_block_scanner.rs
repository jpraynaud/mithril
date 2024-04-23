//! The module used for parsing Cardano transactions
use crate::{
    digesters::ImmutableFile,
    entities::{
        BlockHash, BlockNumber, CardanoTransaction, ImmutableFileNumber, SlotNumber,
        TransactionHash,
    },
    StdResult,
};
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use pallas_hardano::storage::immutable::chunk::{read_blocks, Reader};
use pallas_traverse::MultiEraBlock;
use slog::{debug, error, warn, Logger};
use std::collections::VecDeque;
use std::path::Path;
use tokio::sync::RwLock;

/// A parser that can read cardano transactions in a cardano database
///
/// If you want to mock it using mockall:
/// ```
/// mod test {
///     use anyhow::anyhow;
///     use async_trait::async_trait;
///     use mithril_common::cardano_block_scanner::{BlockScanner, ScannedBlock};
///     use mithril_common::entities::{CardanoDbBeacon, CardanoTransaction, ImmutableFileNumber};
///     use mithril_common::StdResult;
///     use mockall::mock;
///     use std::path::Path;
///
///     mock! {
///         pub BlockScannerImpl { }
///
///         #[async_trait]
///         impl BlockScanner for BlockScannerImpl {
///             async fn scan(
///               &self,
///               dirpath: &Path,
///               from_immutable: Option<ImmutableFileNumber>,
///               until_immutable: ImmutableFileNumber,
///             ) -> StdResult<Box<dyn BlockStreamer>>;
///         }
///     }
///
///     #[test]
///     fn test_mock() {
///         let mut mock = MockBlockScannerImpl::new();
///         mock.expect_scan().return_once(|_, _| {
///             Err(anyhow!("parse error"))
///         });
///     }
/// }
/// ```
#[async_trait]
pub trait BlockScanner: Sync + Send {
    /// Parse the transactions
    async fn scan(
        &self,
        dirpath: &Path,
        from_immutable: Option<ImmutableFileNumber>,
        until_immutable: ImmutableFileNumber,
    ) -> StdResult<Box<dyn BlockStreamer>>;
}

/// Trait that define how blocks are streamed from a Cardano database
#[async_trait]
pub trait BlockStreamer: Sync + Send {
    /// Stream the next available blocks
    async fn poll_next(&mut self) -> StdResult<Option<Vec<ScannedBlock>>>;

    /// Stream all the available blocks, may be very memory intensive
    async fn poll_all(&mut self) -> StdResult<Vec<ScannedBlock>> {
        let mut blocks = Vec::new();
        while let Some(mut next_blocks) = self.poll_next().await? {
            blocks.append(&mut next_blocks);
        }
        Ok(blocks)
    }
}

/// [Block streamer][BlockStreamer] that streams blocks immutable files per immutable files
pub struct ImmutableBlockStreamer {
    remaining_immutables: VecDeque<ImmutableFile>,
    current_immutable_file: Option<ImmutableFile>,
    allow_unparsable_block: bool,
    logger: Logger,
}

#[async_trait]
impl BlockStreamer for ImmutableBlockStreamer {
    async fn poll_next(&mut self) -> StdResult<Option<Vec<ScannedBlock>>> {
        match &self.current_immutable_file {
            Some(immutable_file) => {
                debug!(
                    self.logger,
                    "Reading blocks from immutable file: '{}'",
                    immutable_file.path.display()
                );

                let blocks = self
                    .read_blocks_from_immutable_file(immutable_file)
                    .with_context(|| {
                        format!(
                            "BlockStreamer failed to read blocks from immutable file: '{}'.",
                            immutable_file.path.display()
                        )
                    })?;
                self.current_immutable_file = self.remaining_immutables.pop_front();
                Ok(Some(blocks))
            }
            None => Ok(None),
        }
    }
}

impl ImmutableBlockStreamer {
    /// Factory
    pub fn new(
        immutables_to_stream: Vec<ImmutableFile>,
        allow_unparsable_block: bool,
        logger: Logger,
    ) -> Self {
        let (remaining_immutables, current_immutable_file) = if immutables_to_stream.is_empty() {
            (VecDeque::new(), None)
        } else {
            let mut remaining_immutables = VecDeque::from(immutables_to_stream);
            let current_immutable_file = remaining_immutables.pop_front();
            (remaining_immutables, current_immutable_file)
        };

        Self {
            remaining_immutables,
            current_immutable_file,
            allow_unparsable_block,
            logger,
        }
    }

    /// Read blocks from immutable file
    fn read_blocks_from_immutable_file(
        &self,
        immutable_file: &ImmutableFile,
    ) -> StdResult<Vec<ScannedBlock>> {
        let cardano_blocks_reader = Self::cardano_blocks_reader(immutable_file)?;

        let mut blocks = Vec::new();
        for parsed_block in cardano_blocks_reader {
            let block = parsed_block.with_context(|| {
                format!(
                    "Error while reading block in immutable file: '{:?}'",
                    immutable_file.path
                )
            })?;
            match Self::convert_to_block(&block, immutable_file) {
                Ok(convert_to_block) => {
                    blocks.push(convert_to_block);
                }
                Err(err) if self.allow_unparsable_block => {
                    error!(
                        self.logger,
                        "The cbor encoded block could not be parsed";
                        "error" => ?err, "immutable_file_number" => immutable_file.number
                    );
                }
                Err(e) => return Err(e),
            }
        }
        Ok(blocks)
    }

    fn convert_to_block(block: &[u8], immutable_file: &ImmutableFile) -> StdResult<ScannedBlock> {
        let multi_era_block = MultiEraBlock::decode(block).with_context(|| {
            format!(
                "Error while decoding block in immutable file: '{:?}'",
                immutable_file.path
            )
        })?;

        Ok(ScannedBlock::convert(
            multi_era_block,
            immutable_file.number,
        ))
    }

    fn cardano_blocks_reader(immutable_file: &ImmutableFile) -> StdResult<Reader> {
        let dir_path = immutable_file.path.parent().ok_or(anyhow!(format!(
            "Could not retrieve immutable file directory with immutable file path: '{:?}'",
            immutable_file.path
        )))?;
        let file_name = &Path::new(&immutable_file.filename)
            .file_stem()
            .ok_or(anyhow!(format!(
                "Could not extract immutable file name from file: '{}'",
                immutable_file.filename
            )))?
            .to_string_lossy();
        let blocks = read_blocks(dir_path, file_name)?;

        Ok(blocks)
    }
}

/// Dumb Block Scanner
pub struct DumbBlockScanner {
    blocks: RwLock<Vec<ScannedBlock>>,
}

impl DumbBlockScanner {
    /// Factory
    pub fn new(blocks: Vec<ScannedBlock>) -> Self {
        Self {
            blocks: RwLock::new(blocks),
        }
    }

    /// Update transactions returned by `parse`
    pub async fn update_transactions(&self, new_blocks: Vec<ScannedBlock>) {
        let mut blocks = self.blocks.write().await;
        *blocks = new_blocks;
    }
}

#[async_trait]
impl BlockScanner for DumbBlockScanner {
    async fn scan(
        &self,
        _dirpath: &Path,
        _from_immutable: Option<ImmutableFileNumber>,
        _until_immutable: ImmutableFileNumber,
    ) -> StdResult<Box<dyn BlockStreamer>> {
        // let iter = self.blocks.read().await.clone().into_iter();
        // Ok(Box::new(iter))
        todo!()
    }
}

/// A block scanned from a Cardano database
#[derive(Debug, Clone)]
pub struct ScannedBlock {
    /// Block hash
    pub block_hash: BlockHash,
    /// Block number
    pub block_number: BlockNumber,
    /// Slot number of the block
    pub slot_number: SlotNumber,
    /// Number of the immutable that own the block
    pub immutable_file_number: ImmutableFileNumber,
    /// Hashes of the transactions in the block
    pub transactions: Vec<TransactionHash>,
}

impl ScannedBlock {
    /// Scanned block factory
    pub fn new<T: Into<TransactionHash>, U: Into<BlockHash>>(
        block_hash: U,
        block_number: BlockNumber,
        slot_number: SlotNumber,
        immutable_file_number: ImmutableFileNumber,
        transaction_hashes: Vec<T>,
    ) -> Self {
        Self {
            block_hash: block_hash.into(),
            block_number,
            slot_number,
            immutable_file_number,
            transactions: transaction_hashes.into_iter().map(|h| h.into()).collect(),
        }
    }

    fn convert(multi_era_block: MultiEraBlock, immutable_file_number: ImmutableFileNumber) -> Self {
        let mut transactions = Vec::new();
        for tx in &multi_era_block.txs() {
            transactions.push(tx.hash().to_string());
        }

        Self::new(
            multi_era_block.hash().to_string(),
            multi_era_block.number(),
            multi_era_block.slot(),
            immutable_file_number,
            transactions,
        )
    }

    /// Number of transactions in the block
    pub fn transaction_len(&self) -> usize {
        self.transactions.len()
    }

    /// Convert the scanned block into a list of Cardano transactions.
    ///
    /// Consume the block.
    pub fn into_transactions(self) -> Vec<CardanoTransaction> {
        self.transactions
            .into_iter()
            .map(|transaction_hash| {
                CardanoTransaction::new(
                    transaction_hash,
                    self.block_number,
                    self.slot_number,
                    self.block_hash.clone(),
                    self.immutable_file_number,
                )
            })
            .collect::<Vec<_>>()
    }
}

/// Cardano block scanner
pub struct CardanoBlockScanner {
    logger: Logger,
    /// When set to true, no error is returned in case of unparsable block, and an error log is written instead.
    /// This can occur when the crate 'pallas-hardano' doesn't support some non final encoding for a Cardano era.
    /// This situation should only happen on the test networks and not on the mainnet.
    allow_unparsable_block: bool,
}

impl CardanoBlockScanner {
    /// Factory
    pub fn new(logger: Logger, allow_unparsable_block: bool) -> Self {
        if allow_unparsable_block {
            warn!(
                logger,
                "The 'allow_unparsable_block' option is activated. This option should only be used on test networks.")
        }

        Self {
            logger,
            allow_unparsable_block,
        }
    }
}

#[async_trait]
impl BlockScanner for CardanoBlockScanner {
    async fn scan(
        &self,
        dirpath: &Path,
        from_immutable: Option<ImmutableFileNumber>,
        until_immutable: ImmutableFileNumber,
    ) -> StdResult<Box<dyn BlockStreamer>> {
        let is_in_bounds = |number: ImmutableFileNumber| match from_immutable {
            Some(from) => (from..=until_immutable).contains(&number),
            None => number <= until_immutable,
        };
        let immutable_chunks = ImmutableFile::list_completed_in_dir(dirpath)?
            .into_iter()
            .filter(|f| is_in_bounds(f.number) && f.filename.contains("chunk"))
            .collect::<Vec<_>>();

        Ok(Box::new(ImmutableBlockStreamer::new(
            immutable_chunks,
            self.allow_unparsable_block,
            self.logger.clone(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use slog::Drain;
    use std::{fs::File, sync::Arc};

    use crate::test_utils::{logger_for_tests, TempDir};

    use super::*;

    fn get_number_of_immutable_chunk_in_dir(dir: &Path) -> usize {
        ImmutableFile::list_completed_in_dir(dir)
            .unwrap()
            .into_iter()
            .map(|i| i.filename.contains("chunk"))
            .len()
    }

    fn create_file_logger(filepath: &Path) -> Logger {
        let writer = File::create(filepath).unwrap();
        let decorator = slog_term::PlainDecorator::new(writer);
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        Logger::root(Arc::new(drain), slog::o!())
    }

    #[tokio::test]
    async fn test_parse_expected_number_of_transactions() {
        // We know the number of transactions in those prebuilt immutables
        let immutable_files = [("00000", 0usize), ("00001", 2), ("00002", 3)];
        let db_path = Path::new("../mithril-test-lab/test_data/immutable/");
        assert!(get_number_of_immutable_chunk_in_dir(db_path) >= 3);

        let until_immutable_file = 2;
        // let expected_tx_count: usize = immutable_files.iter().map(|(_, count)| *count).sum();
        let cardano_transaction_parser = CardanoBlockScanner::new(logger_for_tests(), false);

        let mut streamer = cardano_transaction_parser
            .scan(db_path, None, until_immutable_file)
            .await
            .unwrap();

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert_eq!(
            immutable_blocks.map(|b| b.into_iter().map(|b| b.transaction_len()).sum()),
            Some(immutable_files[0].1)
        );

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert_eq!(
            immutable_blocks.map(|b| b.into_iter().map(|b| b.transaction_len()).sum()),
            Some(immutable_files[1].1)
        );

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert_eq!(
            immutable_blocks.map(|b| b.into_iter().map(|b| b.transaction_len()).sum()),
            Some(immutable_files[2].1)
        );

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert!(immutable_blocks.is_none());
    }

    #[tokio::test]
    async fn test_parse_from_lower_bound_until_upper_bound() {
        // We know the number of transactions in those prebuilt immutables
        let immutable_files = [("00002", 3)];
        let db_path = Path::new("../mithril-test-lab/test_data/immutable/");
        assert!(get_number_of_immutable_chunk_in_dir(db_path) >= 3);

        let until_immutable_file = 2;
        let expected_tx_count: usize = immutable_files.iter().map(|(_, count)| *count).sum();
        let cardano_transaction_parser = CardanoBlockScanner::new(logger_for_tests(), false);

        let mut streamer = cardano_transaction_parser
            .scan(db_path, Some(2), until_immutable_file)
            .await
            .unwrap();

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert_eq!(
            immutable_blocks.map(|b| b.into_iter().map(|b| b.transaction_len()).sum()),
            Some(expected_tx_count)
        );
    }

    #[tokio::test]
    async fn test_parse_should_error_with_unparsable_block_format() {
        let db_path = Path::new("../mithril-test-lab/test_data/parsing_error/immutable/");
        let until_immutable_file = 4831;
        let cardano_transaction_parser = CardanoBlockScanner::new(logger_for_tests(), false);

        let mut streamer = cardano_transaction_parser
            .scan(db_path, None, until_immutable_file)
            .await
            .unwrap();
        let result = streamer.poll_all().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parse_should_log_error_with_unparsable_block_format() {
        let temp_dir = TempDir::create(
            "cardano_transaction_parser",
            "test_parse_should_log_error_with_unparsable_block_format",
        );
        let filepath = temp_dir.join("test.log");
        let db_path = Path::new("../mithril-test-lab/test_data/parsing_error/immutable/");
        let until_immutable_file = 4831;
        // We create a block to drop the logger and force a flush before we read the log file.
        {
            let cardano_transaction_parser =
                CardanoBlockScanner::new(create_file_logger(&filepath), true);

            let mut streamer = cardano_transaction_parser
                .scan(db_path, None, until_immutable_file)
                .await
                .unwrap();
            let res = streamer.poll_all().await;
            assert!(res.is_err(), "parse should have failed");
        }

        let log_file = std::fs::read_to_string(&filepath).unwrap();
        assert!(log_file.contains("The cbor encoded block could not be parsed"));
    }

    #[tokio::test]
    async fn test_parse_up_to_given_beacon() {
        // We know the number of transactions in those prebuilt immutables
        let immutable_files = [("00000", 0usize), ("00001", 2)];
        let db_path = Path::new("../mithril-test-lab/test_data/immutable/");
        assert!(get_number_of_immutable_chunk_in_dir(db_path) >= 2);

        let until_immutable_file = 1;
        let cardano_transaction_parser = CardanoBlockScanner::new(logger_for_tests(), false);

        let mut streamer = cardano_transaction_parser
            .scan(db_path, None, until_immutable_file)
            .await
            .unwrap();

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert_eq!(
            immutable_blocks.map(|b| b.into_iter().map(|b| b.transaction_len()).sum()),
            Some(immutable_files[0].1)
        );

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert_eq!(
            immutable_blocks.map(|b| b.into_iter().map(|b| b.transaction_len()).sum()),
            Some(immutable_files[1].1)
        );

        let immutable_blocks = streamer.poll_next().await.unwrap();
        assert!(immutable_blocks.is_none());
    }

    #[tokio::test]
    async fn test_instantiate_parser_with_allow_unparsable_block_should_log_warning() {
        let temp_dir = TempDir::create(
            "cardano_transaction_parser",
            "test_instantiate_parser_with_allow_unparsable_block_should_log_warning",
        );
        let filepath = temp_dir.join("test.log");
        // We create a block to drop the logger and force a flush before we read the log file.
        {
            let _ = CardanoBlockScanner::new(create_file_logger(&filepath), true);
        }

        let log_file = std::fs::read_to_string(&filepath).unwrap();
        assert!(log_file.contains("The 'allow_unparsable_block' option is activated. This option should only be used on test networks."));
    }

    #[tokio::test]
    async fn test_instantiate_parser_without_allow_unparsable_block_should_not_log_warning() {
        let temp_dir = TempDir::create(
            "cardano_transaction_parser",
            "test_instantiate_parser_without_allow_unparsable_block_should_not_log_warning",
        );
        let filepath = temp_dir.join("test.log");
        // We create a block to drop the logger and force a flush before we read the log file.
        {
            let _ = CardanoBlockScanner::new(create_file_logger(&filepath), false);
        }

        let log_file = std::fs::read_to_string(&filepath).unwrap();
        assert!(!log_file.contains("The 'allow_unparsable_block' option is activated. This option should only be used on test networks."));
    }
}
