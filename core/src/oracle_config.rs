use std::{convert::TryFrom, io::Write};

use crate::datapoint_source::{DataPointSource, ExternalScript};
use ergo_lib::{
    ergotree_ir::chain::address::NetworkAddress,
    ergotree_ir::chain::{address::AddressEncoder, ergo_box::box_value::BoxValue},
    wallet::tx_builder::{self, SUGGESTED_TX_FEE},
};
use log::LevelFilter;
use once_cell::sync;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_ORACLE_CONFIG_FILE_NAME: &str = "oracle_config.yaml";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OracleConfig {
    pub node_ip: String,
    pub node_port: u16,
    pub node_api_key: String,
    pub base_fee: u64,
    pub log_level: Option<LevelFilter>,
    pub core_api_port: u16,
    pub oracle_address: NetworkAddress,
    pub data_point_source_custom_script: Option<String>,
}

impl OracleConfig {
    pub fn write_default_config_file() {
        let config = OracleConfig::default();
        let yaml_str = serde_yaml::to_string(&config).unwrap();
        let file_path = ORACLE_CONFIG_FILE_PATH.get().unwrap();
        let mut file = std::fs::File::create(file_path).unwrap();
        file.write_all(yaml_str.as_bytes()).unwrap();
    }

    pub fn load() -> Result<Self, OracleConfigFileError> {
        let config_file_path = ORACLE_CONFIG_FILE_PATH.get().ok_or_else(|| {
            OracleConfigFileError::IoError("ORACLE_CONFIG_FILE_PATH not set".to_string())
        })?;
        let config_str: &str = &std::fs::read_to_string(config_file_path)
            .map_err(|e| OracleConfigFileError::IoError(e.to_string()))?;
        serde_yaml::from_str(config_str)
            .map_err(|e| OracleConfigFileError::ParseError(e.to_string()))
    }

    pub fn save(&self) -> Result<(), OracleConfigFileError> {
        let config_file_path = ORACLE_CONFIG_FILE_PATH.get().ok_or_else(|| {
            OracleConfigFileError::IoError("ORACLE_CONFIG_FILE_PATH not set".to_string())
        })?;
        let yaml_str = serde_yaml::to_string(self).unwrap();
        let mut file = std::fs::File::create(config_file_path).unwrap();
        file.write_all(yaml_str.as_bytes()).unwrap();
        Ok(())
    }

    pub fn custom_data_point_source(&self) -> Option<Box<dyn DataPointSource + Send + Sync>> {
        if let Some(external_script_name) = self.data_point_source_custom_script.clone() {
            Some(Box::new(ExternalScript::new(external_script_name.clone())))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum OracleConfigFileError {
    #[error("Error reading oracle config file: {0}")]
    IoError(String),
    #[error("Error parsing oracle config file: {0}")]
    ParseError(String),
}

impl Default for OracleConfig {
    fn default() -> Self {
        let address = AddressEncoder::unchecked_parse_network_address_from_str(
            "9hEQHEMyY1K1vs79vJXFtNjr2dbQbtWXF99oVWGJ5c4xbcLdBsw",
        )
        .unwrap();
        Self {
            oracle_address: address,
            node_ip: "127.0.0.1".into(),
            node_port: 9053,
            node_api_key: "hello".into(),
            core_api_port: 9010,
            data_point_source_custom_script: None,
            base_fee: *tx_builder::SUGGESTED_TX_FEE().as_u64(),
            log_level: LevelFilter::Info.into(),
        }
    }
}

pub static ORACLE_CONFIG_FILE_PATH: sync::OnceCell<String> = sync::OnceCell::new();
lazy_static! {
    pub static ref ORACLE_CONFIG: OracleConfig = OracleConfig::load().unwrap();
    pub static ref MAYBE_ORACLE_CONFIG: Result<OracleConfig, OracleConfigFileError> =
        OracleConfig::load();
    pub static ref BASE_FEE: BoxValue = MAYBE_ORACLE_CONFIG
        .as_ref()
        .map(|c| BoxValue::try_from(c.base_fee).unwrap())
        .unwrap_or_else(|_| SUGGESTED_TX_FEE());
}

/// Returns "core_api_port" from the config file
pub fn get_core_api_port() -> String {
    ORACLE_CONFIG.core_api_port.to_string()
}

pub fn get_node_ip() -> String {
    ORACLE_CONFIG.node_ip.clone()
}

pub fn get_node_port() -> String {
    ORACLE_CONFIG.node_port.to_string()
}

/// Returns the `node_api_key`
pub fn get_node_api_key() -> String {
    ORACLE_CONFIG.node_api_key.clone()
}
