use derive_more::From;
use ergo_lib::ergotree_ir::chain::address::NetworkAddress;
use ergo_lib::ergotree_ir::chain::token::TokenId;
use ergo_lib::ergotree_ir::ergo_tree::ErgoTree;
use ergo_lib::ergotree_ir::ergo_tree::ErgoTreeConstantError;
use ergo_lib::ergotree_ir::mir::constant::TryExtractFromError;
use ergo_lib::ergotree_ir::mir::constant::TryExtractInto;
use ergo_lib::ergotree_ir::serialization::SigmaParsingError;
use thiserror::Error;

#[derive(Clone)]
pub struct OracleContract {
    ergo_tree: ErgoTree,
    pool_nft_index: usize,
}

#[derive(Debug, From, Error)]
pub enum OracleContractError {
    #[error("oracle contract: failed to get pool NFT from constants")]
    NoPoolNftId,
    #[error("oracle contract: unknown pool NFT defined in constant")]
    UnknownPoolNftId,
    #[error("oracle contract: sigma parsing error {0}")]
    SigmaParsing(SigmaParsingError),
    #[error("oracle contract: ergo tree constant error {0:?}")]
    ErgoTreeConstant(ErgoTreeConstantError),
    #[error("oracle contract: TryExtractFrom error {0:?}")]
    TryExtractFrom(TryExtractFromError),
}

impl OracleContract {
    pub fn load(
        contract_parameters: &OracleContractParameters,
    ) -> Result<Self, OracleContractError> {
        let ergo_tree = contract_parameters.p2s.address().script()?;
        let contract = Self::from_ergo_tree(ergo_tree, contract_parameters)?;
        Ok(contract)
    }

    pub fn create(
        contract_parameters: &OracleContractParameters,
    ) -> Result<Self, OracleContractError> {
        let ergo_tree = contract_parameters.p2s.address().script()?.with_constant(
            contract_parameters.pool_nft_index,
            contract_parameters.pool_nft_token_id.clone().into(),
        )?;
        let contract = Self::from_ergo_tree(ergo_tree, contract_parameters)?;
        Ok(contract)
    }

    /// Create new contract from existing ergo tree, returning error if parameters differ.
    pub fn from_ergo_tree(
        ergo_tree: ErgoTree,
        contract_parameters: &OracleContractParameters,
    ) -> Result<Self, OracleContractError> {
        dbg!(ergo_tree.get_constants().unwrap());

        let pool_nft_token_id = ergo_tree
            .get_constant(contract_parameters.pool_nft_index)
            .map_err(|_| OracleContractError::NoPoolNftId)?
            .ok_or(OracleContractError::NoPoolNftId)?
            .try_extract_into::<TokenId>()?;
        if pool_nft_token_id != contract_parameters.pool_nft_token_id {
            return Err(OracleContractError::UnknownPoolNftId);
        }

        Ok(Self {
            ergo_tree,
            pool_nft_index: contract_parameters.pool_nft_index,
        })
    }

    pub fn ergo_tree(&self) -> ErgoTree {
        self.ergo_tree.clone()
    }

    pub fn pool_nft_token_id(&self) -> TokenId {
        self.ergo_tree
            .get_constant(self.pool_nft_index)
            .unwrap()
            .unwrap()
            .try_extract_into::<TokenId>()
            .unwrap()
    }
}

#[derive(Debug, Clone)]
/// Parameters for the oracle contract
pub struct OracleContractParameters {
    pub p2s: NetworkAddress,
    pub pool_nft_index: usize,
    pub pool_nft_token_id: TokenId,
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_constant_parsing() {
        let contract_parameters = OracleContractParameters::default();
        let c = OracleContract::load(&contract_parameters).unwrap();
        assert_eq!(c.pool_nft_token_id(), contract_parameters.pool_nft_token_id,);
    }
}
