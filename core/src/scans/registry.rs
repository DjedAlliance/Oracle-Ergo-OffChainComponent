use std::path::PathBuf;

use crate::node_interface::node_api::NodeApi;
use crate::node_interface::node_api::NodeApiError;
use crate::pool_config::POOL_CONFIG;
use crate::spec_token::OracleTokenId;

use ::serde::Deserialize;
use ::serde::Serialize;
use once_cell::sync;
use thiserror::Error;

use super::generic_token_scan::GenericTokenScan;
use super::NodeScan;
use super::ScanError;

pub static SCANS_DIR_PATH: sync::OnceCell<PathBuf> = sync::OnceCell::new();

pub fn get_scans_file_path() -> PathBuf {
    SCANS_DIR_PATH.get().unwrap().join("scanIDs.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeScanRegistry {
    #[serde(rename = "All Datapoints Scan")]
    pub oracle_token_scan: GenericTokenScan<OracleTokenId>,
}

impl NodeScanRegistry {
    fn load_from_json_str(json_str: &str) -> Result<Self, NodeScanRegistryError> {
        serde_json::from_str(json_str).map_err(|e| NodeScanRegistryError::Parse(e.to_string()))
    }

    fn save_to_json_str(&self) -> String {
        serde_json::to_string_pretty(&self).unwrap()
    }

    fn save_to_json_file(&self, file_path: &PathBuf) -> Result<(), NodeScanRegistryError> {
        let json_str = self.save_to_json_str();
        log::debug!("Saving scan IDs to {}", file_path.display());
        std::fs::write(file_path, json_str).map_err(|e| NodeScanRegistryError::Io(e.to_string()))
    }

    fn register_and_save_scans_inner(
        node_api: &NodeApi,
    ) -> std::result::Result<Self, NodeScanRegistryError> {
        let pool_config = &POOL_CONFIG;
        log::info!("Registering UTXO-Set Scans");
        let oracle_token_scan =
            GenericTokenScan::register(node_api, &pool_config.token_ids.oracle_token_id)?;
        let registry = Self { oracle_token_scan };
        registry.save_to_json_file(&get_scans_file_path())?;
        node_api.rescan_from_height(0)?;
        Ok(registry)
    }

    pub fn load() -> Result<Self, NodeScanRegistryError> {
        let path = get_scans_file_path();
        log::debug!("Loading scan IDs from {}", path.display());
        let json_str =
            std::fs::read_to_string(path).map_err(|e| NodeScanRegistryError::Io(e.to_string()))?;
        let registry = Self::load_from_json_str(&json_str)?;
        Ok(registry)
    }

    pub fn ensure_node_registered_scans(
        node_api: &NodeApi,
    ) -> std::result::Result<Self, NodeScanRegistryError> {
        let path = get_scans_file_path();
        log::debug!("Loading scan IDs from {}", path.display());
        let registry = if let Ok(json_str) = std::fs::read_to_string(path) {
            Self::load_from_json_str(&json_str)?
        } else {
            Self::register_and_save_scans_inner(node_api)?
        };
        wait_for_node_rescan(node_api)?;
        Ok(registry)
    }

    fn node_scans(&self) -> Vec<&dyn NodeScan> {
        vec![&self.oracle_token_scan]
    }

    pub fn deregister_all_scans(self, node_api: &NodeApi) -> Result<(), NodeApiError> {
        for scan in self.node_scans() {
            node_api.deregister_scan(scan.scan_id())?;
        }
        Ok(())
    }
}

fn wait_for_node_rescan(node_api: &NodeApi) -> Result<(), NodeApiError> {
    let wallet_height = node_api.node.wallet_status()?.height;
    let block_height = node_api.node.current_block_height()?;
    if wallet_height == block_height {
        return Ok(());
    }
    Ok(loop {
        let wallet_height = node_api.node.wallet_status()?.height;
        let block_height = node_api.node.current_block_height()?;
        println!("Scanned {}/{} blocks", wallet_height, block_height);
        if wallet_height == block_height {
            println!("Wallet Scan Complete!");
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    })
}

#[derive(Debug, Error)]
pub enum NodeScanRegistryError {
    #[error("Error registering scan: {0}")]
    Scan(#[from] ScanError),
    #[error("Error node: {0}")]
    NodeApi(#[from] NodeApiError),
    #[error("Error parsing oracle config file: {0}")]
    Parse(String),
    #[error("Error reading/writing file: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use crate::scans::NodeScanId;

    use super::*;
    use ergo_node_interface::ScanId;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_legacy_json() {
        let json_str = r#"{ 
        "All Datapoints Scan": "185",
        "Update Box Scan": "186",
        "Pool Box Scan": "187",
        "Refresh Box Scan": "188",
        "Local Oracle Datapoint Scan": "189",
        "Local Ballot Box Scan": "190",
        "Ballot Box Scan": "191" 
        }"#;
        let registry = NodeScanRegistry::load_from_json_str(json_str).unwrap();
        assert_eq!(registry.oracle_token_scan.scan_id(), ScanId::from(185));
    }

    #[test]
    fn check_encoded_json() {
        let registry = NodeScanRegistry {
            oracle_token_scan: GenericTokenScan::new(ScanId::from(185)),
        };
        let json_str = registry.save_to_json_str();
        let expected_json_str = r#"{
  "All Datapoints Scan": "185"
}"#;
        assert_eq!(json_str, expected_json_str);
    }

    #[test]
    fn json_roundtrip() {
        let registry = NodeScanRegistry {
            oracle_token_scan: GenericTokenScan::new(ScanId::from(185)),
        };
        let json_str = registry.save_to_json_str();
        let registry2 = NodeScanRegistry::load_from_json_str(&json_str).unwrap();
        assert_eq!(registry, registry2);
    }
}
