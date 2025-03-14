#![allow(non_camel_case_types)]

use core::panic;
use frame_support::{
    ensure,
    pallet_prelude::ConstU32,
    parameter_types,
    traits::{ConstBool, ConstU8, Currency, Everything, Get, Hooks},
    PalletId,
};
use frame_system::{
    self as system,
    offchain::{AppCrypto, SigningTypes},
};
use pallet_governance::GlobalGovernanceConfig;
use pallet_governance_api::*;
use pallet_emission_api::{SubnetConsensus, SubnetEmissionApi};
use pallet_chain::{
    params::subnet::SubnetChangeset, Address, DefaultKey, DefaultSubnetParams, Dividends, Emission,
    Incentive, LastUpdate, MaxRegistrationsPerBlock, Name, StakeFrom, StakeTo, SubnetBurn,
    SubnetParams, Tempo, TotalStake, Uids, N,
};
use parity_scale_codec::{Decode, Encode};
use rand::rngs::OsRng;
use rsa::{traits::PublicKeyParts, Pkcs1v15Encrypt};
use scale_info::{prelude::collections::BTreeSet, TypeInfo};

use sp_core::{sr25519, ConstU16, ConstU64, H256};
use sp_runtime::{
    testing::TestXt,
    traits::{
        AccountIdConversion, BlakeTwo256, Extrinsic as ExtrinsicT, IdentifyAccount, IdentityLookup,
    },
    BuildStorage, DispatchError, DispatchResult, KeyTypeId,
};
use std::{
    cell::RefCell,
    io::{Cursor, Read},
};

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        SubnetEmissionMod: pallet_emission,
        ChainMod: pallet_chain,
        GovernanceMod: pallet_governance,
    }
);

pub type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u32;

#[allow(dead_code)]
pub type BalanceCall = pallet_balances::Call<Test>;

#[allow(dead_code)]
pub type TestRuntimeCall = frame_system::Call<Test>;

parameter_types! {
    pub const BlockHashCount: u64 = 250;
    pub const SS58Prefix: u8 = 42;
}

// Balance of an account.
pub type Balance = u64;

// An index to a block.
#[allow(dead_code)]
pub type BlockNumber = u64;

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}

pub const PALLET_ID: PalletId = PalletId(*b"py/subsp");

pub struct ChainPalletId;

impl Get<PalletId> for ChainPalletId {
    fn get() -> PalletId {
        PALLET_ID
    }
}

thread_local! {
    static DEFAULT_MODULE_MIN_BURN: RefCell<u64> = RefCell::new(10_000_000_000);
    static DEFAULT_SUBNET_MIN_BURN: RefCell<u64> = RefCell::new(2_000_000_000_000);
    static DEFAULT_MIN_VALIDATOR_STAKE: RefCell<u64> = RefCell::new(50_000_000_000_000);
}

pub struct ModuleMinBurnConfig;
pub struct SubnetMinBurnConfig;
pub struct MinValidatorStake;

impl Get<u64> for ModuleMinBurnConfig {
    fn get() -> u64 {
        DEFAULT_MODULE_MIN_BURN.with(|v| *v.borrow())
    }
}

impl Get<u64> for SubnetMinBurnConfig {
    fn get() -> u64 {
        DEFAULT_SUBNET_MIN_BURN.with(|v| *v.borrow())
    }
}

impl Get<u64> for MinValidatorStake {
    fn get() -> u64 {
        DEFAULT_MIN_VALIDATOR_STAKE.with(|v| *v.borrow())
    }
}

pub fn set_default_module_min_burn(value: u64) {
    DEFAULT_MODULE_MIN_BURN.with(|v| *v.borrow_mut() = value);
}

pub fn set_default_subnet_min_burn(value: u64) {
    DEFAULT_SUBNET_MIN_BURN.with(|v| *v.borrow_mut() = value);
}

pub fn set_default_min_validator_stake(value: u64) {
    DEFAULT_MIN_VALIDATOR_STAKE.with(|v| *v.borrow_mut() = value)
}

impl pallet_chain::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type WeightInfo = ();
    type DefaultMaxRegistrationsPerInterval = ConstU16<{ u16::MAX }>;
    type DefaultMaxSubnetRegistrationsPerInterval = ConstU16<{ u16::MAX }>;
    type DefaultModuleMinBurn = ModuleMinBurnConfig;
    type DefaultSubnetMinBurn = SubnetMinBurnConfig;
    type DefaultMinValidatorStake = MinValidatorStake;
    type PalletId = ChainPalletId;
    type EnforceWhitelist = ConstBool<false>;
    type DefaultUseWeightsEncryption = ConstBool<false>;
}

impl GovernanceApi<<Test as frame_system::Config>::AccountId> for Test {
    fn get_dao_treasury_address() -> AccountId {
        ChainPalletId::get().into_account_truncating()
    }

    fn is_delegating_voting_power(_delegator: &AccountId) -> bool {
        false
    }

    fn update_delegating_voting_power(_delegator: &AccountId, _delegating: bool) -> DispatchResult {
        Ok(())
    }

    fn get_global_governance_configuration() -> GovernanceConfiguration {
        Default::default()
    }

    fn get_subnet_governance_configuration(_subnet_id: u16) -> GovernanceConfiguration {
        Default::default()
    }

    fn update_global_governance_configuration(config: GovernanceConfiguration) -> DispatchResult {
        pallet_governance::Pallet::<Test>::update_global_governance_configuration(config)
    }

    fn update_subnet_governance_configuration(
        subnet_id: u16,
        config: GovernanceConfiguration,
    ) -> DispatchResult {
        pallet_governance::Pallet::<Test>::update_subnet_governance_configuration(subnet_id, config)
    }

    fn execute_application(user_id: &AccountId) -> DispatchResult {
        pallet_governance::Pallet::<Test>::execute_application(user_id)
    }

    fn get_general_subnet_application_cost() -> u64 {
        to_nano(1_000)
    }

    fn curator_application_exists(module_key: &<Test as frame_system::Config>::AccountId) -> bool {
        pallet_governance::Pallet::<Test>::curator_application_exists(module_key)
    }

    fn whitelisted_keys() -> BTreeSet<AccountId> {
        Default::default()
    }

    fn get_curator() -> <Test as frame_system::Config>::AccountId {
        AccountId::default()
    }

    fn set_curator(_key: &<Test as frame_system::Config>::AccountId) {}

    fn set_general_subnet_application_cost(_amount: u64) {}

    fn clear_subnet_includes(netuid: u16) {
        for storage_type in pallet_governance::SubnetIncludes::all() {
            storage_type.remove_storage::<Test>(netuid);
        }
    }
}

impl SubnetEmissionApi<<Test as frame_system::Config>::AccountId> for Test {
    fn get_lowest_emission_netuid(ignore_subnet_immunity: bool) -> Option<u16> {
        pallet_emission::Pallet::<Test>::get_lowest_emission_netuid(ignore_subnet_immunity)
    }

    fn set_emission_storage(subnet_id: u16, emission: u64) {
        pallet_emission::Pallet::<Test>::set_emission_storage(subnet_id, emission)
    }

    fn create_yuma_subnet(netuid: u16) {
        pallet_emission::Pallet::<Test>::create_yuma_subnet(netuid)
    }

    fn can_remove_subnet(netuid: u16) -> bool {
        pallet_emission::Pallet::<Test>::can_remove_subnet(netuid)
    }

    fn is_mineable_subnet(netuid: u16) -> bool {
        pallet_emission::Pallet::<Test>::is_mineable_subnet(netuid)
    }

    fn get_consensus_netuid(subnet_consensus: SubnetConsensus) -> Option<u16> {
        pallet_emission::Pallet::<Test>::get_consensus_netuid(subnet_consensus)
    }

    fn get_subnet_consensus_type(
        netuid: u16,
    ) -> Option<pallet_emission_api::SubnetConsensus> {
        pallet_emission::SubnetConsensusType::<Test>::get(netuid)
    }

    fn set_subnet_consensus_type(
        netuid: u16,
        subnet_consensus: Option<pallet_emission_api::SubnetConsensus>,
    ) {
        pallet_emission::SubnetConsensusType::<Test>::set(netuid, subnet_consensus)
    }

    fn get_weights(netuid: u16, uid: u16) -> Option<Vec<(u16, u16)>> {
        pallet_emission::Weights::<Test>::get(netuid, uid)
    }

    fn clear_subnet_includes(netuid: u16) {
        for storage_type in pallet_emission::SubnetIncludes::all() {
            storage_type.remove_storage::<Test>(netuid);
        }
    }

    fn set_weights(
        netuid: u16,
        uid: u16,
        weights: Option<Vec<(u16, u16)>>,
    ) -> Option<Vec<(u16, u16)>> {
        let old_weights = pallet_emission::Weights::<Test>::get(netuid, uid);
        pallet_emission::Weights::<Test>::set(netuid, uid, weights);
        old_weights
    }

    fn clear_module_includes(
        netuid: u16,
        uid: u16,
        replace_uid: u16,
        module_key: &<Test as frame_system::Config>::AccountId,
        replace_key: &<Test as frame_system::Config>::AccountId,
    ) -> DispatchResult {
        pallet_emission::StorageHandler::remove_all::<Test>(
            netuid,
            uid,
            replace_uid,
            &module_key,
            &replace_key,
        )?;
        Ok(())
    }
}

impl pallet_emission::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type Decimals = Decimals;
    type HalvingInterval = HalvingInterval;
    type MaxSupply = MaxSupply;
    type DecryptionNodeRotationInterval = ConstU64<5_000>;
    type MaxAuthorities = ConstU32<100>;
    type OffchainWorkerBanDuration = ConstU64<10_800>;
    // setting high value because of testing (this should never be reached in practice)
    type MissedPingsForInactivity = ConstU8<{ u8::MAX }>;
    type PingInterval = ConstU64<50>;
    type EncryptionPeriodBuffer = ConstU64<100>;
    type WeightInfo = ();
}

impl pallet_governance::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = ChainPalletId;
    type WeightInfo = ();
}

impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AccountStore = System;
    type Balance = u64;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type MaxLocks = MaxLocks;
    type WeightInfo = ();
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = ();
    type RuntimeHoldReason = ();
    type FreezeIdentifier = ();
    type MaxFreezes = frame_support::traits::ConstU32<16>;
    type RuntimeFreezeReason = ();
}

// Things needed to impl offchain worker module
// ============================================

pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"offw");

pub struct TestAuthId;

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Encode, Decode, TypeInfo)]
pub struct CustomPublic(sr25519::Public);

impl IdentifyAccount for CustomPublic {
    type AccountId = u32;

    fn into_account(self) -> Self::AccountId {
        let bytes: &[u8] = self.0.as_ref();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

impl From<sr25519::Public> for CustomPublic {
    fn from(pub_key: sr25519::Public) -> Self {
        CustomPublic(pub_key)
    }
}

impl From<CustomPublic> for sr25519::Public {
    fn from(custom_public: CustomPublic) -> Self {
        custom_public.0
    }
}

pub type Extrinsic = TestXt<RuntimeCall, ()>;

impl frame_system::offchain::SigningTypes for Test {
    type Public = CustomPublic;
    type Signature = sp_core::sr25519::Signature;
}

impl<LocalCall> frame_system::offchain::SendTransactionTypes<LocalCall> for Test
where
    RuntimeCall: From<LocalCall>,
{
    type OverarchingCall = RuntimeCall;
    type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Test
where
    RuntimeCall: From<LocalCall>,
{
    fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
        call: RuntimeCall,
        _public: Self::Public,
        _account: AccountId,
        nonce: u64,
    ) -> Option<(RuntimeCall, <Extrinsic as ExtrinsicT>::SignaturePayload)> {
        Some((call, (nonce, ())))
    }
}

impl system::Config for Test {
    type BaseCallFilter = Everything;
    type Block = Block;
    type BlockWeights = ();
    type BlockLength = ();
    type AccountId = AccountId;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type Lookup = IdentityLookup<Self::AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeOrigin = RuntimeOrigin;
    type BlockHashCount = BlockHashCount;
    type DbWeight = ();
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = SS58Prefix;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;

    type RuntimeTask = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

pub struct MockEmissionConfig {
    pub decimals: u8,
    pub halving_interval: u64,
    pub max_supply: u64,
}

#[allow(dead_code)]
const TOKEN_DECIMALS: u32 = 9;

#[allow(dead_code)]
pub const fn to_nano(x: u64) -> u64 {
    x * 10u64.pow(TOKEN_DECIMALS)
}

#[allow(dead_code)]
pub const fn from_nano(x: u64) -> u64 {
    x / 10u64.pow(TOKEN_DECIMALS)
}

impl Default for MockEmissionConfig {
    fn default() -> Self {
        Self {
            decimals: TOKEN_DECIMALS as u8,
            halving_interval: 250_000_000,
            max_supply: 1_000_000_000,
        }
    }
}

pub struct Decimals;
impl Get<u8> for Decimals {
    fn get() -> u8 {
        MOCK_EMISSION_CONFIG.with(|config| config.borrow().decimals)
    }
}

pub struct HalvingInterval;
impl Get<u64> for HalvingInterval {
    fn get() -> u64 {
        MOCK_EMISSION_CONFIG.with(|config| config.borrow().halving_interval)
    }
}

pub struct MaxSupply;
impl Get<u64> for MaxSupply {
    fn get() -> u64 {
        MOCK_EMISSION_CONFIG.with(|config| config.borrow().max_supply)
    }
}

thread_local! {
    static MOCK_EMISSION_CONFIG: RefCell<MockEmissionConfig> = RefCell::new(MockEmissionConfig::default());
}

#[allow(dead_code)]
pub fn set_emission_config(decimals: u8, halving_interval: u64, max_supply: u64) {
    MOCK_EMISSION_CONFIG.with(|config| {
        *config.borrow_mut() = MockEmissionConfig {
            decimals,
            halving_interval,
            max_supply,
        };
    });
}

pub fn add_balance(key: AccountId, amount: Balance) {
    let _ = <Balances as Currency<AccountId>>::deposit_creating(&key, amount);
}

pub fn delegate(account: u32) {
    assert_ok!(GovernanceMod::enable_vote_power_delegation(get_origin(
        account
    )));
}

#[allow(dead_code)]
pub fn get_stakes(netuid: u16) -> Vec<u64> {
    get_uid_key_tuples(netuid)
        .into_iter()
        .map(|(_, key)| ChainMod::get_delegated_stake(&key))
        .collect()
}

pub fn stake(account: u32, module: u32, stake: u64) {
    if get_balance(account) <= stake {
        add_balance(account, stake + to_nano(1));
    }

    assert_ok!(ChainMod::do_add_stake(
        get_origin(account),
        module,
        stake
    ));
}

pub fn increase_stake(key: AccountId, stake: u64) {
    ChainMod::increase_stake(&key, &key, stake);
}

// Sets all key's stake to 0 and increases delegated stake to desired amount
pub fn make_keys_all_stake_be(account: AccountId, stake: u64) {
    let _ = StakeFrom::<Test>::clear_prefix(&account, u32::MAX, None);
    let _ = StakeTo::<Test>::clear_prefix(&account, u32::MAX, None);

    let keys_total_stake =
        ChainMod::get_delegated_stake(&account) + ChainMod::get_owned_stake(&account);

    TotalStake::<Test>::mutate(|total_stake| {
        *total_stake = total_stake.saturating_sub(keys_total_stake).saturating_add(stake);
    });

    increase_stake(account, stake);
}

pub fn set_total_issuance(total_issuance: u64) {
    let key = DefaultKey::<Test>::get();
    // Reset the issuance (completelly nuke the key's balance)
    <Test as pallet_chain::Config>::Currency::make_free_balance_be(&key, 0u32.into());
    // Add the total_issuance to the key's balance
    ChainMod::add_balance_to_account(&key, total_issuance);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    sp_tracing::try_init_simple();
    let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {});
    ext
}

pub fn new_test_ext_with_block(block: u64) -> sp_io::TestExternalities {
    sp_tracing::try_init_simple();
    let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(block));
    ext
}

pub fn get_origin(key: AccountId) -> RuntimeOrigin {
    <<Test as frame_system::Config>::RuntimeOrigin>::signed(key)
}

#[allow(dead_code)]
pub fn get_total_subnet_balance(netuid: u16) -> u64 {
    let keys = get_keys(netuid);
    keys.iter().map(ChainMod::get_balance_u64).sum()
}

#[allow(dead_code)]
pub(crate) fn step_block(n: u16) {
    for _ in 0..n {
        ChainMod::on_finalize(System::block_number());
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        ChainMod::on_initialize(System::block_number());
        SubnetEmissionMod::on_initialize(System::block_number());
        GovernanceMod::on_initialize(System::block_number());
    }
}

#[allow(dead_code)]
pub(crate) fn run_to_block(n: u64) {
    while System::block_number() < n {
        ChainMod::on_finalize(System::block_number());
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        ChainMod::on_initialize(System::block_number());
        SubnetEmissionMod::on_initialize(System::block_number());
        GovernanceMod::on_initialize(System::block_number());
    }
}

#[allow(dead_code)]
pub(crate) fn step_epoch(netuid: u16) {
    let tempo: u16 = Tempo::<Test>::get(netuid);
    step_block(tempo);
}

#[allow(dead_code)]
pub fn set_weights(netuid: u16, key: AccountId, uids: Vec<u16>, values: Vec<u16>) {
    SubnetEmissionMod::set_weights(get_origin(key), netuid, uids.clone(), values.clone()).unwrap();
}

#[allow(dead_code)]
pub fn set_weights_encrypted(
    netuid: u16,
    key: AccountId,
    encrypted_weights: Vec<u8>,
    decrypted_weights_hash: Vec<u8>,
    set_last_updated: bool,
) {
    SubnetEmissionMod::do_set_weights_encrypted(
        get_origin(key),
        netuid,
        encrypted_weights,
        decrypted_weights_hash,
        set_last_updated,
    )
    .unwrap();
}

#[allow(dead_code)]
pub fn get_stake_for_uid(netuid: u16, module_uid: u16) -> u64 {
    let Some(key) = ChainMod::get_key_for_uid(netuid, module_uid) else {
        return 0;
    };
    ChainMod::get_delegated_stake(&key)
}

#[allow(dead_code)]
pub fn get_emission_for_key(netuid: u16, key: &AccountId) -> u64 {
    let uid = ChainMod::get_uid_for_key(netuid, key).unwrap();
    ChainMod::get_emission_for(netuid)
        .get(uid as usize)
        .copied()
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn get_uids(netuid: u16) -> Vec<u16> {
    (0..N::<Test>::get(netuid)).collect()
}

#[allow(dead_code)]
pub fn get_keys(netuid: u16) -> Vec<AccountId> {
    get_uids(netuid)
        .into_iter()
        .map(|uid| ChainMod::get_key_for_uid(netuid, uid).unwrap())
        .collect()
}

#[allow(dead_code)]
pub fn get_uid_key_tuples(netuid: u16) -> Vec<(u16, AccountId)> {
    (0..N::<Test>::get(netuid))
        .map(|uid| (uid, ChainMod::get_key_for_uid(netuid, uid).unwrap()))
        .collect()
}

#[allow(dead_code)]
pub fn get_names(netuid: u16) -> Vec<Vec<u8>> {
    Name::<Test>::iter_prefix(netuid).map(|(_, name)| name).collect()
}

#[allow(dead_code)]
pub fn get_addresses(netuid: u16) -> Vec<AccountId> {
    Uids::<Test>::iter_prefix(netuid).map(|(key, _)| key).collect()
}

pub fn delegate_register_module(
    netuid: u16,
    key: AccountId,
    module_key: AccountId,
    stake: u64,
) -> DispatchResult {
    // can i format the test in rus

    let origin = get_origin(key);
    let network = format!("test{netuid}").as_bytes().to_vec();
    let name = format!("module{module_key}").as_bytes().to_vec();
    let address = "0.0.0.0:30333".as_bytes().to_vec();

    let is_new_subnet = !ChainMod::if_subnet_exist(netuid);
    if is_new_subnet {
        MaxRegistrationsPerBlock::<Test>::set(1000)
    }

    let balance = ChainMod::get_balance(&key);
    if stake >= balance {
        ChainMod::add_balance_to_account(&key, stake + 1);
    }

    let _ = ChainMod::register_subnet(origin.clone(), network.clone(), None);
    let result = ChainMod::register(origin, network, name.clone(), address, module_key, None);
    ChainMod::increase_stake(&key, &module_key, stake);

    log::info!("Register ok module: network: {name:?}, module_key: {module_key} key: {key}");

    result
}

#[allow(dead_code)]
pub fn check_subnet_storage(netuid: u16) -> bool {
    let n = N::<Test>::get(netuid);
    let uids = get_uids(netuid);
    let keys = get_keys(netuid);
    let names = get_names(netuid);
    let addresses = get_addresses(netuid);
    let emissions = Emission::<Test>::get(netuid);
    let incentives = Incentive::<Test>::get(netuid);
    let dividends = Dividends::<Test>::get(netuid);
    let last_update = LastUpdate::<Test>::get(netuid);

    if (n as usize) != uids.len() {
        return false;
    }
    if (n as usize) != keys.len() {
        return false;
    }
    if (n as usize) != names.len() {
        return false;
    }
    if (n as usize) != addresses.len() {
        return false;
    }
    if (n as usize) != emissions.len() {
        return false;
    }
    if (n as usize) != incentives.len() {
        return false;
    }
    if (n as usize) != dividends.len() {
        return false;
    }
    if (n as usize) != last_update.len() {
        return false;
    }

    // length of addresss
    let name_vector: Vec<Vec<u8>> = Name::<Test>::iter_prefix_values(netuid).collect();
    if (n as usize) != name_vector.len() {
        return false;
    }

    // length of addresss
    let address_vector: Vec<Vec<u8>> = Address::<Test>::iter_prefix_values(netuid).collect();
    if (n as usize) != address_vector.len() {
        return false;
    }

    true
}

#[allow(dead_code)]
pub fn register_n_modules(netuid: u16, n: u16, stake: u64, increase_emission: bool) {
    for i in 0..n {
        register_module(netuid, i as u32, stake, increase_emission).unwrap_or_else(|err| {
            panic!(
                "register module failed for netuid: {netuid} key: {i} stake: {stake}. err: {err:?}"
            )
        });
    }
}

pub fn register_named_subnet(key: AccountId, netuid: u16, name: impl ToString) -> DispatchResult {
    ensure!(
        !N::<Test>::contains_key(netuid),
        "subnet id already registered"
    );

    let name = name.to_string().as_bytes().to_vec();
    let params = SubnetParams {
        name: name.clone().try_into().unwrap(),
        founder: key,
        ..DefaultSubnetParams::<Test>::get()
    };
    Test::set_subnet_consensus_type(netuid, Some(SubnetConsensus::Yuma));
    ChainMod::add_subnet(SubnetChangeset::<Test>::new(params).unwrap(), Some(netuid)).unwrap();

    Ok(())
}

pub fn register_subnet(key: AccountId, netuid: u16) -> DispatchResult {
    register_named_subnet(key, netuid, format!("test{netuid}"))
}

#[allow(dead_code)]
pub fn register_module(
    netuid: u16,
    key: AccountId,
    stake: u64,
    increase_emission: bool,
) -> Result<u16, DispatchError> {
    let origin = get_origin(key);

    let network = format!("test{netuid}").as_bytes().to_vec();
    let name = format!("module{key}").as_bytes().to_vec();
    let address = "0.0.0.0:30333".as_bytes().to_vec();

    let _ = register_subnet(key, netuid);

    ChainMod::add_balance_to_account(&key, SubnetBurn::<Test>::get() + 1);
    let _ = ChainMod::register_subnet(origin.clone(), network.clone(), None);
    ChainMod::register(origin, network.clone(), name, address, key, None)?;
    ChainMod::increase_stake(&key, &key, stake);

    let netuid = ChainMod::get_netuid_for_name(&network).ok_or("netuid is missing")?;
    let uid = pallet_chain::Uids::<Test>::get(netuid, key).ok_or("uid is missing")?;

    if increase_emission {
        Emission::<Test>::mutate(netuid, |v| v[uid as usize] = stake);
        pallet_emission::SubnetEmission::<Test>::mutate(netuid, |s| *s += stake);
    }

    Ok(uid)
}

#[allow(dead_code)]
pub fn register_root_validator(key: AccountId, stake: u64) -> Result<u16, DispatchError> {
    let origin = get_origin(key);
    let network = b"Rootnet".to_vec();
    let name = format!("module{key}").as_bytes().to_vec();
    let address = "0.0.0.0:30333".as_bytes().to_vec();

    let _ = ChainMod::register_subnet(origin.clone(), network.clone(), None);
    ChainMod::register(origin, network.clone(), name, address, key, None)?;
    ChainMod::increase_stake(&key, &key, stake);

    let netuid = ChainMod::get_netuid_for_name(&network).ok_or("netuid is missing")?;
    if netuid != 0 {
        return Err("rootnet id is not 0".into());
    }
    pallet_chain::Uids::<Test>::get(netuid, key).ok_or("uid is missing".into())
}

pub fn get_total_issuance() -> u64 {
    let total_staked_balance = TotalStake::<Test>::get();
    let total_free_balance = pallet_balances::Pallet::<Test>::total_issuance();
    total_staked_balance + total_free_balance
}

pub fn vote(account: u32, proposal_id: u64, agree: bool) {
    assert_ok!(GovernanceMod::do_vote_proposal(
        get_origin(account),
        proposal_id,
        agree
    ));
}

pub fn config(proposal_cost: u64, proposal_expiration: u32) {
    GlobalGovernanceConfig::<Test>::set(GovernanceConfiguration {
        proposal_cost,
        proposal_expiration,
        vote_mode: pallet_governance_api::VoteMode::Vote,
        ..Default::default()
    });
}

// Utility functions
//===================

pub fn get_balance(key: AccountId) -> Balance {
    <Balances as Currency<AccountId>>::free_balance(&key)
}

#[allow(dead_code)]
pub fn round_first_five(num: u64) -> u64 {
    let place_value = 10_u64.pow(num.to_string().len() as u32 - 5);
    let first_five = num / place_value;

    if first_five % 10 >= 5 {
        (first_five / 10 + 1) * place_value * 10
    } else {
        (first_five / 10) * place_value * 10
    }
}

#[allow(dead_code)]
pub fn zero_min_burn() {
    set_default_module_min_burn(0);
    set_default_subnet_min_burn(0);
}

#[allow(dead_code)]
pub fn zero_min_validator_stake() {
    set_default_min_validator_stake(0);
}

macro_rules! update_params {
    ($netuid:expr => { $($f:ident: $v:expr),+ }) => {{
        let params = ::pallet_chain::SubnetParams {
            $($f: $v),+,
            ..ChainMod::subnet_params($netuid)
        };
        ::pallet_chain::params::subnet::SubnetChangeset::<Test>::update($netuid, params).unwrap().apply($netuid).unwrap();
    }};
    ($netuid:expr => $params:expr) => {{
        ::pallet_chain::subnet::SubnetChangeset::<Test>::update($netuid, $params).unwrap().apply($netuid).unwrap();
    }};
}

macro_rules! assert_ok {
    ( $x:expr $(,)? ) => {
        match $x {
            Ok(v) => v,
            is => panic!("Expected Ok(_). Got {is:#?}"),
        }
    };
    ( $x:expr, $y:expr $(,)? ) => {
        assert_eq!($x, Ok($y));
    };
}

macro_rules! assert_in_range {
    ($value:expr, $expected:expr, $margin:expr) => {
        assert!(
            ($expected - $margin..=$expected + $margin).contains(&$value),
            "value {} is out of range {}..={}",
            $value,
            $expected,
            $margin
        );
    };
}

pub(crate) use assert_in_range;
pub(crate) use assert_ok;
pub(crate) use update_params;

// TEMP

struct Decrypter {
    // TODO: swap this with the node's decryption key type and store it once it starts
    key: Option<rsa::RsaPrivateKey>,
}

impl Default for Decrypter {
    fn default() -> Self {
        Self {
            key: Some(rsa::RsaPrivateKey::new(&mut OsRng, 1024).unwrap()),
        }
    }
}



fn read_u32(cursor: &mut Cursor<&Vec<u8>>) -> Option<u32> {
    let mut buf: [u8; 4] = [0u8; 4];
    match cursor.read_exact(&mut buf[..]) {
        Ok(()) => Some(u32::from_be_bytes(buf)),
        Err(err) => {
            dbg!(&err);
            None
        }
    }
}

fn read_u16(cursor: &mut Cursor<&Vec<u8>>) -> Option<u16> {
    let mut buf = [0u8; 2];
    match cursor.read_exact(&mut buf[..]) {
        Ok(()) => Some(u16::from_be_bytes(buf)),
        Err(err) => {
            dbg!(&err);
            None
        }
    }
}
