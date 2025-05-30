use jsonrpsee::{
    core::{ClientError as JsonRpseeError, RpcResult},
    proc_macros::rpc,
    types::error::ErrorObject,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{
    traits::{Block as BlockT, IdentifyAccount, Verify},
    MultiSignature,
};
use std::sync::Arc;
use chain_runtime_api::ModuleInfo;
pub use chain_runtime_api::ChainRuntimeApi;

type Signature = MultiSignature;
type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Custom {
    code: u32,
}

#[rpc(client, server)]
pub trait ChainApi<BlockHash> {
    #[method(name = "chain_getModuleInfo")]
    fn get_module_info(
        &self,
        key: AccountId,
        netuid: u16,
        at: Option<BlockHash>,
    ) -> RpcResult<ModuleInfo>;
}

pub struct ChainPallet<C, Block> {
    client: Arc<C>,
    _marker: std::marker::PhantomData<Block>,
}

impl<C, Block> ChainPallet<C, Block> {
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}

impl<C, Block> ChainApiServer<<Block as BlockT>::Hash> for ChainPallet<C, Block>
where
    Block: BlockT,
    C: Send + Sync + 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
    C::Api: ChainRuntimeApi<Block>,
{
    fn get_module_info(
        &self,
        key: AccountId,
        netuid: u16,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<ModuleInfo> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);

        let value = api.get_module_info(at, key, netuid).map_err(runtime_error_into_rpc_err);
        Ok(value.unwrap())
    }
}

const RUNTIME_ERROR: i32 = 1;

/// Converts a runtime trap into an RPC error.
fn runtime_error_into_rpc_err(err: impl std::fmt::Debug) -> JsonRpseeError {
    JsonRpseeError::Call(ErrorObject::owned(
        RUNTIME_ERROR,
        "Runtime error",
        Some(format!("{:?}", err)),
    ))
}
