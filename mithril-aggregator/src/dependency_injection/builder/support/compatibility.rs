use std::sync::Arc;

use mithril_common::api_version::APIVersionProvider;
use mithril_common::entities::Epoch;
use mithril_common::era::adapters::{EraReaderAdapterBuilder, EraReaderDummyAdapter};
use mithril_common::era::{EraChecker, EraMarker, EraReader, EraReaderAdapter, SupportedEra};

use crate::dependency_injection::{DependenciesBuilder, DependenciesBuilderError, Result};
use crate::ExecutionEnvironment;

impl DependenciesBuilder {
    async fn build_api_version_provider(&mut self) -> Result<Arc<APIVersionProvider>> {
        let api_version_provider = Arc::new(APIVersionProvider::new(self.get_era_checker().await?));

        Ok(api_version_provider)
    }

    /// [APIVersionProvider] service
    pub async fn get_api_version_provider(&mut self) -> Result<Arc<APIVersionProvider>> {
        if self.api_version_provider.is_none() {
            self.api_version_provider = Some(self.build_api_version_provider().await?);
        }

        Ok(self.api_version_provider.as_ref().cloned().unwrap())
    }

    async fn build_era_reader(&mut self) -> Result<Arc<EraReader>> {
        let era_adapter: Arc<dyn EraReaderAdapter> = match self.configuration.environment {
            ExecutionEnvironment::Production => EraReaderAdapterBuilder::new(
                &self.configuration.era_reader_adapter_type,
                &self.configuration.era_reader_adapter_params,
            )
            .build(self.get_chain_observer().await?)
            .map_err(|e| DependenciesBuilderError::Initialization {
                message: "Could not build EraReader as dependency.".to_string(),
                error: Some(e.into()),
            })?,
            _ => Arc::new(EraReaderDummyAdapter::from_markers(vec![EraMarker::new(
                &SupportedEra::dummy().to_string(),
                Some(Epoch(0)),
            )])),
        };

        Ok(Arc::new(EraReader::new(era_adapter)))
    }

    /// [EraReader] service
    pub async fn get_era_reader(&mut self) -> Result<Arc<EraReader>> {
        if self.era_reader.is_none() {
            self.era_reader = Some(self.build_era_reader().await?);
        }

        Ok(self.era_reader.as_ref().cloned().unwrap())
    }

    async fn build_era_checker(&mut self) -> Result<Arc<EraChecker>> {
        let current_epoch = self
            .get_ticker_service()
            .await?
            .get_current_epoch()
            .await
            .map_err(|e| DependenciesBuilderError::Initialization {
                message: "Error while building EraChecker".to_string(),
                error: Some(e),
            })?;
        let era_epoch_token = self
            .get_era_reader()
            .await?
            .read_era_epoch_token(current_epoch)
            .await
            .map_err(|e| DependenciesBuilderError::Initialization {
                message: "Error while building EraChecker".to_string(),
                error: Some(e.into()),
            })?;
        let era_checker = Arc::new(EraChecker::new(
            era_epoch_token.get_current_supported_era().map_err(|e| {
                DependenciesBuilderError::Initialization {
                    message: "Error while building EraChecker".to_string(),
                    error: Some(e),
                }
            })?,
            era_epoch_token.get_current_epoch(),
        ));

        Ok(era_checker)
    }

    /// [EraReader] service
    pub async fn get_era_checker(&mut self) -> Result<Arc<EraChecker>> {
        if self.era_checker.is_none() {
            self.era_checker = Some(self.build_era_checker().await?);
        }

        Ok(self.era_checker.as_ref().cloned().unwrap())
    }
}
