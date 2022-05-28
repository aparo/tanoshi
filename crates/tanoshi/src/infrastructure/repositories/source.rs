use std::str::FromStr;

use async_trait::async_trait;
use serde::Deserialize;
use tanoshi_lib::prelude::Version;
use tanoshi_vm::prelude::SourceBus;

use crate::domain::{
    entities::source::Source,
    repositories::source::{SourceRepository, SourceRepositoryError},
};

#[derive(Deserialize)]
pub struct SourceDto {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub version: String,
    pub rustc_version: String,
    pub lib_version: String,
    pub icon: String,
}

#[derive(Clone)]
pub struct SourceRepositoryImpl {
    ext: SourceBus,
}

impl SourceRepositoryImpl {
    pub fn new(ext: SourceBus) -> Self {
        Self { ext }
    }
}

#[async_trait]
impl SourceRepository for SourceRepositoryImpl {
    async fn installed_sources(&self) -> Result<Vec<Source>, SourceRepositoryError> {
        let mut sources = self
            .ext
            .list()
            .await?
            .into_iter()
            .map(|s| s.into())
            .collect::<Vec<Source>>();

        sources.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(sources)
    }

    async fn available_sources(
        &self,
        repo_url: &str,
    ) -> Result<Vec<Source>, SourceRepositoryError> {
        let source_indexes: Vec<SourceDto> = reqwest::get(&format!("{repo_url}/index.json"))
            .await?
            .json()
            .await?;

        let mut sources: Vec<Source> = vec![];
        for index in source_indexes {
            if !self.ext.exists(index.id).await? {
                sources.push(Source {
                    id: index.id,
                    name: index.name,
                    url: index.url,
                    version: index.version,
                    rustc_version: index.rustc_version,
                    lib_version: index.lib_version,
                    icon: index.icon,
                    has_update: false,
                });
            }
        }
        Ok(sources)
    }

    async fn get_source_by_id(&self, id: i64) -> Result<Source, SourceRepositoryError> {
        let source = self.ext.get_source_info(id)?;
        Ok(source.into())
    }

    async fn install_source(&self, repo_url: &str, id: i64) -> Result<(), SourceRepositoryError> {
        if self.ext.exists(id).await? {
            return Err(SourceRepositoryError::Other(
                "source installed, use updateSource to update".to_string(),
            ));
        }

        let source_indexes: Vec<SourceDto> = reqwest::get(format!("{repo_url}/index.json"))
            .await?
            .json()
            .await?;

        let source = source_indexes
            .iter()
            .find(|index| index.id == id)
            .ok_or_else(|| SourceRepositoryError::NotFound)?
            .clone();

        if source.rustc_version != tanoshi_lib::RUSTC_VERSION
            || source.lib_version != tanoshi_lib::LIB_VERSION
        {
            return Err(SourceRepositoryError::Other(
                "Incompatible version, update tanoshi server".to_string(),
            ));
        }

        self.ext.install(repo_url, &source.name).await?;

        Ok(())
    }

    async fn update_source(&self, repo_url: &str, id: i64) -> Result<(), SourceRepositoryError> {
        let installed_source = self.ext.get_source_info(id)?;

        let source_indexes: Vec<SourceDto> = reqwest::get(format!("{repo_url}/index.json"))
            .await?
            .json()
            .await?;
        let source = source_indexes
            .iter()
            .find(|index| index.id == id)
            .ok_or_else(|| SourceRepositoryError::NotFound)?
            .clone();

        if Version::from_str(installed_source.version)? == Version::from_str(&source.version)? {
            return Err(SourceRepositoryError::Other("No new version".to_string()));
        }

        if source.rustc_version != tanoshi_lib::RUSTC_VERSION
            || source.lib_version != tanoshi_lib::LIB_VERSION
        {
            return Err(SourceRepositoryError::Other(
                "Incompatible version, update tanoshi server".to_string(),
            ));
        }

        self.ext.remove(id).await?;
        self.ext.install(repo_url, &source.name).await?;

        Ok(())
    }

    async fn uninstall_source(&self, id: i64) -> Result<(), SourceRepositoryError> {
        self.ext.remove(id).await?;

        Ok(())
    }
}