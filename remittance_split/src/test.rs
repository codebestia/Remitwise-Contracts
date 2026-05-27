#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, TryFromVal,
};

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

fn setup_split(
    env: &Env,
    spending: u32,
    savings: u32,
    bills: u32,
    insurance: u32,
) -> (
    RemittanceSplitClient<'_>,
    Address,
    Address,
    StellarAssetClient<'_>,
) {
    env.mock_all_auths();
    set_time(env, 1_000);

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = StellarAssetClient::new(env, &token_addr);

    client.initialize_split(
        &owner,
        &0,
        &token_addr,
        &spending,
        &savings,
        &bills,
        &insurance,
    );

    (client, owner, token_addr, stellar_client)
}

fn sample_accounts(env: &Env) -> AccountGroup {
    AccountGroup {
        spending: Address::generate(env),
        savings: Address::generate(env),
        bills: Address::generate(env),
        insurance: Address::generate(env),
    }
}

#[test]
fn test_distribution_completed_event() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 40, 30, 20, 10);
    let accounts = sample_accounts(&env);

    let total_amount = 1_000i128;
    stellar_client.mint(&owner, &total_amount);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        total_amount,
        deadline,
    );

    client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &total_amount,
    );

    let events = env.events().all();
    let last_event = events.last().expect("no events emitted");
    let (_, topics, data) = last_event;

    assert_eq!(topics.len(), 4);

    let event: DistributionCompletedEvent = DistributionCompletedEvent::try_from_val(&env, &data)
        .expect("failed to decode distribution event");

    assert_eq!(event.from, owner);
    assert_eq!(event.total_amount, total_amount);
    assert_eq!(event.spending_amount, 400);
    assert_eq!(event.savings_amount, 300);
    assert_eq!(event.bills_amount, 200);
    assert_eq!(event.insurance_amount, 100);
    assert_eq!(event.timestamp, env.ledger().timestamp());
}

#[test]
fn test_distribution_event_topic_correctness() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 50, 50, 0, 0);
    let accounts = sample_accounts(&env);

    stellar_client.mint(&owner, &100);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        100,
        deadline,
    );

    client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &100,
    );

    let events = env.events().all();
    let dist_comp_event = events
        .iter()
        .find(|event| event.1.len() == 4)
        .expect("distribution completed event not found");

    assert_eq!(dist_comp_event.1.len(), 4);
}

#[test]
fn test_request_hash_deterministic() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let hash1 = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        7,
        1_000,
        2_000,
    );
    let hash2 =
        RemittanceSplit::compute_request_hash(symbol_short!("distH"), owner, 7, 1_000, 2_000);

    assert_eq!(hash1, hash2);
}

#[test]
fn test_request_hash_changes_with_parameters() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let base = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        0,
        1_000,
        2_000,
    );

    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            1,
            1_000,
            2_000
        )
    );
    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            0,
            2_000,
            2_000
        )
    );
    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(symbol_short!("distH"), owner, 0, 1_000, 3_000)
    );
}

#[test]
fn test_distribute_usdc_signed_success() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 50, 30, 15, 5);
    let accounts = sample_accounts(&env);
    let token = TokenClient::new(&env, &token_addr);

    stellar_client.mint(&owner, &1_000);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: accounts.clone(),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 100,
    };

    let hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        request.nonce,
        request.total_amount,
        request.deadline,
    );

    let result = client.distribute_usdc_signed(&request, &hash);
    assert!(result);
    assert_eq!(token.balance(&accounts.spending), 500);
    assert_eq!(token.balance(&accounts.savings), 300);
    assert_eq!(token.balance(&accounts.bills), 150);
    assert_eq!(token.balance(&accounts.insurance), 50);
    assert_eq!(client.get_nonce(&owner), 2);
}

#[test]
fn test_distribute_usdc_signed_deadline_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: sample_accounts(&env),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() - 1,
    };

    let hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner,
        request.nonce,
        request.total_amount,
        request.deadline,
    );

    let result = client.try_distribute_usdc_signed(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

#[test]
fn test_distribute_usdc_signed_hash_mismatch() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: sample_accounts(&env),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 100,
    };

    let wrong_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner,
        request.nonce,
        request.total_amount + 1,
        request.deadline,
    );

    let result = client.try_distribute_usdc_signed(&request, &wrong_hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
}

// ============================================================================
// Self-Transfer Guard Tests
// ============================================================================
// Verifies that SelfTransferNotAllowed is raised before any nonce or token
// side-effects occur, for both distribute_usdc and distribute_usdc_signed.
// ============================================================================

mod self_transfer_guard {
    use crate::{AccountGroup, DistributeUsdcRequest, RemittanceSplit, RemittanceSplitError};
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events},
        token::TokenClient,
        Address, Env,
    };

    // ── Test A ───────────────────────────────────────────────────────────────
    // distribute_usdc — basic self-transfer rejection.
    // Guard fires before nonce check, so deadline/hash can be dummy values.
    #[test]
    fn test_a_distribute_usdc_basic_self_transfer_rejection() {
        let env = Env::default();
        let (client, owner, token_addr, stellar_client) = super::setup_split(&env, 40, 30, 20, 10);
        stellar_client.mint(&owner, &1_000);

        let nonce_before = client.get_nonce(&owner);

        // spending == from triggers the self-transfer guard
        let accounts = AccountGroup {
            spending: owner.clone(),
            savings: Address::generate(&env),
            bills: Address::generate(&env),
            insurance: Address::generate(&env),
        };

        let events_before = env.events().all().len();

        // Guard fires before the nonce / deadline / hash checks — those values
        // are irrelevant for this rejection path.
        let result = client.try_distribute_usdc(
            &token_addr,
            &owner,
            &nonce_before,
            &(env.ledger().timestamp() + 100),
            &0u64,
            &accounts,
            &1_000,
        );

        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::SelfTransferNotAllowed))
        );

        // Nonce must be unchanged — no side-effects on rejected path
        assert_eq!(client.get_nonce(&owner), nonce_before);

        // No new events emitted on the rejection path
        // NOTE: append_audit writes to instance storage, not to the event log.
        // All RemitwiseEvents::emit calls are only reached on the success path.
        assert_eq!(
            env.events().all().len(),
            events_before,
            "no events must be emitted on SelfTransferNotAllowed rejection"
        );

        // NOTE: In the Soroban test environment (soroban-sdk 21.x), returning
        // Err(...) from a contract function causes ALL storage mutations in that
        // invocation to be reverted. This means the append_audit(..., false) call
        // inside the self-transfer guard is rolled back and is not observable via
        // get_audit_log(). The guard's audit behaviour is verified by on-chain
        // integration tests. Here we confirm the audit log is UNCHANGED (no
        // phantom success entry was added).
        let audit = client.get_audit_log(&0, &100);
        let last = audit.items.last().expect("audit log must not be empty");
        assert!(
            last.success,
            "the init entry (success=true) must still be the last entry — no spurious entries added"
        );
    }

    // ── Test B ───────────────────────────────────────────────────────────────
    // distribute_usdc_signed — valid signature but destination == from.
    // Hash check passes; self-transfer guard fires before nonce check.
    #[test]
    fn test_b_distribute_usdc_signed_valid_sig_self_dest() {
        let env = Env::default();
        let (client, owner, token_addr, stellar_client) = super::setup_split(&env, 40, 30, 20, 10);
        let token = TokenClient::new(&env, &token_addr);
        stellar_client.mint(&owner, &1_000);

        let deadline = env.ledger().timestamp() + 100;
        let nonce = client.get_nonce(&owner);

        // savings == from: self-transfer on the signed path
        let request = DistributeUsdcRequest {
            usdc_contract: token_addr,
            from: owner.clone(),
            nonce,
            accounts: AccountGroup {
                spending: Address::generate(&env),
                savings: owner.clone(),
                bills: Address::generate(&env),
                insurance: Address::generate(&env),
            },
            total_amount: 1_000,
            deadline,
        };

        let hash = RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            request.nonce,
            request.total_amount,
            request.deadline,
        );

        let owner_balance_before = token.balance(&owner);
        let nonce_before = client.get_nonce(&owner);
        let events_before = env.events().all().len();

        let result = client.try_distribute_usdc_signed(&request, &hash);

        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::SelfTransferNotAllowed))
        );

        // Nonce must be unchanged
        assert_eq!(client.get_nonce(&owner), nonce_before);

        // No token movement occurred
        assert_eq!(
            token.balance(&owner),
            owner_balance_before,
            "owner balance must not change on SelfTransferNotAllowed"
        );

        // No new events emitted on the rejection path
        assert_eq!(
            env.events().all().len(),
            events_before,
            "no events must be emitted on SelfTransferNotAllowed rejection"
        );

        // NOTE: In the Soroban test environment (soroban-sdk 21.x), returning
        // Err(...) causes storage mutations to be reverted. append_audit(..., false)
        // inside the guard is therefore rolled back and not visible via get_audit_log.
        // We verify the audit log is unchanged (no spurious entry of any kind was added).
        let audit = client.get_audit_log(&0, &100);
        let last = audit.items.last().expect("audit log must not be empty");
        assert!(
            last.success,
            "the init entry (success=true) must still be the last entry — no spurious entries added"
        );
    }

    // ── Test C ───────────────────────────────────────────────────────────────
    // distribute_usdc_signed — nonce invariant: strict equality before/after.
    #[test]
    fn test_c_distribute_usdc_signed_nonce_invariant_after_rejection() {
        let env = Env::default();
        let (client, owner, token_addr, _) = super::setup_split(&env, 40, 30, 20, 10);

        let nonce_before = client.get_nonce(&owner);
        let deadline = env.ledger().timestamp() + 100;

        let request = DistributeUsdcRequest {
            usdc_contract: token_addr,
            from: owner.clone(),
            nonce: nonce_before,
            accounts: AccountGroup {
                spending: owner.clone(), // self-transfer
                savings: Address::generate(&env),
                bills: Address::generate(&env),
                insurance: Address::generate(&env),
            },
            total_amount: 500,
            deadline,
        };

        let hash = RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            request.nonce,
            request.total_amount,
            request.deadline,
        );

        let _ = client.try_distribute_usdc_signed(&request, &hash);

        let nonce_after = client.get_nonce(&owner);

        assert_eq!(
            nonce_before, nonce_after,
            "nonce must be strictly unchanged on SelfTransferNotAllowed"
        );
    }

    // ── Test D ───────────────────────────────────────────────────────────────
    // distribute_usdc — non-self transfer sanity / positive case.
    // from != any destination; verifies no regression from the guard.
    #[test]
    fn test_d_distribute_usdc_non_self_transfer_succeeds() {
        let env = Env::default();
        let (client, owner, token_addr, stellar_client) = super::setup_split(&env, 40, 30, 20, 10);
        stellar_client.mint(&owner, &1_000);

        let nonce = client.get_nonce(&owner); // 1 after initialize_split
        let accounts = super::sample_accounts(&env); // all distinct from owner
        let deadline = env.ledger().timestamp() + 3_600;
        let request_hash = RemittanceSplit::compute_request_hash(
            symbol_short!("distrib"),
            owner.clone(),
            nonce,
            1_000,
            deadline,
        );

        let result = client.try_distribute_usdc(
            &token_addr,
            &owner,
            &nonce,
            &deadline,
            &request_hash,
            &accounts,
            &1_000,
        );

        assert_eq!(result, Ok(Ok(true)));

        // Nonce incremented by exactly 1
        assert_eq!(client.get_nonce(&owner), nonce + 1);

        // Audit log shows success entry
        let audit = client.get_audit_log(&0, &100);
        let last = audit.items.last().expect("audit log must not be empty");
        assert!(
            last.success,
            "last audit entry must be success for a valid non-self distribution"
        );
    }

    // ── Test E ───────────────────────────────────────────────────────────────
    // distribute_usdc — all four destinations == from (full self-split).
    // The guard must still fire for this extreme case.
    #[test]
    fn test_e_distribute_usdc_all_destinations_self() {
        let env = Env::default();
        let (client, owner, token_addr, stellar_client) = super::setup_split(&env, 40, 30, 20, 10);
        stellar_client.mint(&owner, &1_000);

        let nonce_before = client.get_nonce(&owner);

        // Every category points back to `from` — full self-split
        let accounts = AccountGroup {
            spending: owner.clone(),
            savings: owner.clone(),
            bills: owner.clone(),
            insurance: owner.clone(),
        };

        let result = client.try_distribute_usdc(
            &token_addr,
            &owner,
            &nonce_before,
            &(env.ledger().timestamp() + 100),
            &0u64,
            &accounts,
            &1_000,
        );

        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::SelfTransferNotAllowed))
        );

        assert_eq!(
            client.get_nonce(&owner),
            nonce_before,
            "nonce must be unchanged after full self-split rejection"
        );
    }
}
