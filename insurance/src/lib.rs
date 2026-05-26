#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use remitwise_common::CoverageType;
use soroban_sdk::{
    contract, contractimpl, contracterror, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InsuranceError {
    PolicyNotFound = 1,
    Unauthorized = 2,
    PolicyInactive = 3,
    InvalidExternalRef = 4,
    DuplicateExternalRef = 5,
}

/// Event emitted by `set_external_ref` on every successful external-reference change. Carries the old and new ref values for off-chain indexers.
#[contracttype]
#[derive(Clone)]
pub struct ExternalRefUpdatedEvent {
    pub policy_id: u32,
    pub old_external_ref: Option<String>,
    pub new_external_ref: Option<String>,
    pub timestamp: u64,
}

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518_400; // ~30 days

// Pagination constants (used by tests)
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

// Storage keys
const KEY_PAUSE_ADMIN: Symbol = symbol_short!("PAUSE_ADM");
const KEY_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
const KEY_POLICIES: Symbol = symbol_short!("POLICIES");
const KEY_OWNER_INDEX: Symbol = symbol_short!("OWN_IDX");
/// Instance-storage key for the external-reference index. Holds a `Map<String, u32>` mapping each active `external_ref` string to its owning policy ID.
const KEY_EXT_REF_IDX: Symbol = symbol_short!("EXT_IDX");

// Event topic constants
/// Event topic symbol emitted by `set_external_ref` on every successful ref change. Payload is `ExternalRefUpdatedEvent`.
const EVT_EXT_REF_UPDATED: Symbol = symbol_short!("ext_upd");

#[contracttype]
#[derive(Clone)]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyPage {
    pub items: Vec<InsurancePolicy>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        }
    }

    /// Validates that `ext_ref` is between 1 and 128 bytes (inclusive).
    /// Returns `Err(InsuranceError::InvalidExternalRef)` if the length is 0 or > 128.
    fn validate_external_ref(ext_ref: &String) -> Result<(), InsuranceError> {
        let len = ext_ref.len();
        if len == 0 || len > 128 {
            return Err(InsuranceError::InvalidExternalRef);
        }
        Ok(())
    }

    /// Reads `KEY_EXT_REF_IDX` from instance storage and returns the policy ID
    /// mapped to `ext_ref`, or `None` if no mapping exists.
    fn ext_idx_get(env: &Env, ext_ref: &String) -> Option<u32> {
        let idx: Map<String, u32> = env
            .storage()
            .instance()
            .get(&KEY_EXT_REF_IDX)
            .unwrap_or_else(|| Map::new(env));
        idx.get(ext_ref.clone())
    }

    /// Loads `KEY_EXT_REF_IDX` (or creates a new empty map), inserts the
    /// `(ext_ref → policy_id)` mapping, and saves it back to instance storage.
    fn ext_idx_insert(env: &Env, ext_ref: &String, policy_id: u32) {
        let mut idx: Map<String, u32> = env
            .storage()
            .instance()
            .get(&KEY_EXT_REF_IDX)
            .unwrap_or_else(|| Map::new(env));
        idx.set(ext_ref.clone(), policy_id);
        env.storage().instance().set(&KEY_EXT_REF_IDX, &idx);
    }

    /// Loads `KEY_EXT_REF_IDX` (or creates a new empty map), removes the entry
    /// for `ext_ref`, and saves it back to instance storage.
    fn ext_idx_remove(env: &Env, ext_ref: &String) {
        let mut idx: Map<String, u32> = env
            .storage()
            .instance()
            .get(&KEY_EXT_REF_IDX)
            .unwrap_or_else(|| Map::new(env));
        idx.remove(ext_ref.clone());
        env.storage().instance().set(&KEY_EXT_REF_IDX, &idx);
    }

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);
        env.storage().instance().set(&KEY_PAUSE_ADMIN, &new_admin);
        true
    }

    /// Creates a new insurance policy.
    ///
    /// # Errors
    /// - `InsuranceError::InvalidExternalRef` — if `external_ref` is `Some` but empty or longer than 128 bytes.
    /// - `InsuranceError::DuplicateExternalRef` — if `external_ref` is `Some` and already held by an active policy.
    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        external_ref: Option<String>,
    ) -> Result<u32, InsuranceError> {
        owner.require_auth();
        Self::extend_instance_ttl(&env);

        let mut next_id: u32 = env.storage().instance().get(&KEY_NEXT_ID).unwrap_or(0);
        next_id += 1;

        if let Some(ref r) = external_ref {
            Self::validate_external_ref(r)?;
        }

        if let Some(ref r) = external_ref {
            if Self::ext_idx_get(&env, r).is_some() {
                return Err(InsuranceError::DuplicateExternalRef);
            }
        }

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let policy = InsurancePolicy {
            id: next_id,
            owner: owner.clone(),
            name,
            external_ref: external_ref.clone(),
            coverage_type,
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date: env.ledger().timestamp() + (30 * 86_400),
        };
        policies.set(next_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        let mut index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));
        let mut ids = index.get(owner.clone()).unwrap_or_else(|| Vec::new(&env));
        ids.push_back(next_id);
        index.set(owner, ids);
        env.storage().instance().set(&KEY_OWNER_INDEX, &index);

        if let Some(ref r) = external_ref {
            Self::ext_idx_insert(&env, r, next_id);
        }

        env.storage().instance().set(&KEY_NEXT_ID, &next_id);
        Ok(next_id)
    }

    pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
        Self::extend_instance_ttl(&env);
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        policies.get(policy_id)
    }

    /// Looks up the policy ID currently mapped to `ext_ref` in `EXT_IDX`.
    ///
    /// # Security invariant
    /// This function only returns IDs for active policies. Entries are removed from `EXT_IDX`
    /// when a policy is deactivated or archived, so this function will never return a stale ID.
    ///
    /// # Stability invariant
    /// While a policy is active and its `external_ref` has not been changed, this function
    /// returns the same `Some(policy_id)` on every call.
    pub fn get_policy_id_by_external_ref(env: Env, ext_ref: String) -> Option<u32> {
        Self::extend_instance_ttl(&env);
        Self::ext_idx_get(&env, &ext_ref)
    }

    /// Atomically updates a policy's `external_ref` and re-indexes `EXT_IDX`.
    ///
    /// - Removes the old `external_ref` from `EXT_IDX` (if `Some`).
    /// - Inserts the new `external_ref` into `EXT_IDX` (if `Some`).
    /// - If `new_ref` equals the current `external_ref`, returns `Ok(true)` immediately
    ///   without modifying storage or emitting an event (idempotent).
    /// - Emits `ExternalRefUpdatedEvent` (topic `EVT_EXT_REF_UPDATED`) on every successful change.
    ///
    /// # Errors
    /// - `InsuranceError::PolicyNotFound` — policy does not exist.
    /// - `InsuranceError::Unauthorized` — caller is not the policy owner.
    /// - `InsuranceError::PolicyInactive` — policy is not active.
    /// - `InsuranceError::InvalidExternalRef` — `new_ref` is `Some` but empty or > 128 bytes.
    /// - `InsuranceError::DuplicateExternalRef` — `new_ref` is already held by another active policy.
    pub fn set_external_ref(
        env: Env,
        caller: Address,
        policy_id: u32,
        new_ref: Option<String>,
    ) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return Err(InsuranceError::PolicyNotFound),
        };

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }

        // Idempotent: if new_ref equals current ref, return immediately
        if new_ref == policy.external_ref {
            return Ok(true);
        }

        // Validate new ref length
        if let Some(ref r) = new_ref {
            Self::validate_external_ref(r)?;
        }

        // Duplicate check: skip the current policy's own entry
        if let Some(ref r) = new_ref {
            if let Some(existing_id) = Self::ext_idx_get(&env, r) {
                if existing_id != policy_id {
                    return Err(InsuranceError::DuplicateExternalRef);
                }
            }
        }

        let old_ref = policy.external_ref.clone();

        // Remove old entry from index
        if let Some(ref r) = old_ref {
            Self::ext_idx_remove(&env, r);
        }

        // Insert new entry into index
        if let Some(ref r) = new_ref {
            Self::ext_idx_insert(&env, r, policy_id);
        }

        // Update policy record
        policy.external_ref = new_ref.clone();
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        // Emit event
        let event = ExternalRefUpdatedEvent {
            policy_id,
            old_external_ref: old_ref,
            new_external_ref: new_ref,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((EVT_EXT_REF_UPDATED,), event);

        Ok(true)
    }

    /// Deactivates a policy, setting `active = false` and removing its `external_ref` from `EXT_IDX`.
    /// Returns `Ok(false)` if the policy does not exist or the caller is not the owner.
    pub fn deactivate_policy(env: Env, caller: Address, policy_id: u32) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return Ok(false),
        };
        if policy.owner != caller {
            return Ok(false);
        }
        policy.active = false;
        policies.set(policy_id, policy.clone());
        env.storage().instance().set(&KEY_POLICIES, &policies);
        if let Some(ref r) = policy.external_ref {
            Self::ext_idx_remove(&env, r);
        }
        Ok(true)
    }

    /// Permanently removes a policy from active service and frees its `external_ref` for reuse.
    /// Removes the policy from `KEY_POLICIES` and removes its `external_ref` from `EXT_IDX`.
    /// Returns `Ok(false)` if the policy does not exist. Returns `Err(InsuranceError::Unauthorized)` if the caller is not the owner.
    pub fn archive_policy(env: Env, caller: Address, policy_id: u32) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return Ok(false),
        };

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        if let Some(ref r) = policy.external_ref {
            Self::ext_idx_remove(&env, r);
        }

        policies.remove(policy_id);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        Ok(true)
    }

    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != caller || !policy.active {
            return false;
        }
        policy.next_payment_date = env.ledger().timestamp() + (30 * 86_400);
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);
        true
    }

    pub fn batch_pay_premiums(env: Env, caller: Address, policy_ids: Vec<u32>) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let mut count: u32 = 0;
        let next_date = env.ledger().timestamp() + (30 * 86_400);
        for id in policy_ids.iter() {
            if let Some(mut p) = policies.get(id) {
                if p.owner == caller && p.active {
                    p.next_payment_date = next_date;
                    policies.set(id, p);
                    count += 1;
                }
            }
        }
        env.storage().instance().set(&KEY_POLICIES, &policies);
        count
    }

    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        Self::extend_instance_ttl(&env);

        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));

        let ids = index.get(owner).unwrap_or_else(|| Vec::new(&env));
        let mut total: i128 = 0;
        for id in ids.iter() {
            if let Some(p) = policies.get(id) {
                if p.active {
                    total += p.monthly_premium;
                }
            }
        }
        total
    }

    /// Returns a stable, cursor-based page of active policies for an owner.
    pub fn get_active_policies(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> PolicyPage {
        Self::extend_instance_ttl(&env);
        let limit = Self::clamp_limit(limit);

        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));
        let ids = index.get(owner).unwrap_or_else(|| Vec::new(&env));

        let mut items: Vec<InsurancePolicy> = Vec::new(&env);
        let mut next_cursor: u32 = 0;

        for id in ids.iter() {
            if id <= cursor {
                continue;
            }
            if let Some(p) = policies.get(id) {
                if !p.active {
                    continue;
                }
                items.push_back(p);
                next_cursor = id;
                if items.len() >= limit {
                    break;
                }
            }
        }

        // If we returned a full page, we may or may not have more items;
        // keep the cursor as the last returned id (caller can continue).
        // If we returned less than a full page, no more data -> cursor 0.
        let out_cursor = if items.len() < limit { 0 } else { next_cursor };

        let count = items.len();
        PolicyPage {
            items,
            next_cursor: out_cursor,
            count,
        }
    }
}

mod test;
