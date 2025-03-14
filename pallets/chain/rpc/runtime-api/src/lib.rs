#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_arithmetic::per_things::Percent;

use sp_runtime::{
    sp_std::{collections::btree_map::BTreeMap, prelude::Vec},
    traits::{IdentifyAccount, Verify},
    MultiSignature,
};

type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

#[derive(Decode, Encode, PartialEq, Eq, Clone, Debug, TypeInfo, Serialize, Deserialize)]
pub struct ModuleStats {
    pub last_update: u64,
    pub registration_block: u64,
    /// A map of AccountId to stake amount (in u64) for this module/key.
    /// This includes both direct stakes and delegations.
    pub stake_from: BTreeMap<AccountId, u64>,
    pub emission: u64,
    pub incentive: u16,
    pub dividends: u16,
    pub weights: Vec<(u16, u16)>, // Vec of (uid, weight)
}

#[derive(Decode, Encode, PartialEq, Eq, Clone, Debug, TypeInfo, Serialize, Deserialize)]
pub struct ModuleParams {
    pub name: Vec<u8>,
    pub address: Vec<u8>,
    pub delegation_fee: Percent,
    pub metadata: Option<Vec<u8>>,
}

#[derive(Decode, Encode, PartialEq, Eq, Clone, Debug, TypeInfo, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub params: ModuleParams,
    pub stats: ModuleStats,
}

sp_api::decl_runtime_apis! {
    pub trait ChainRuntimeApi {
        fn get_module_info(key: AccountId, netuid: u16) -> ModuleInfo;
    }
}
