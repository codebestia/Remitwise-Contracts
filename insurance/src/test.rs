#[cfg(test)]
mod tests {
    use crate::*;
    use proptest::prelude::*;
    use remitwise_common::CoverageType;
    use soroban_sdk::testutils::Address as AddressTrait;
    use soroban_sdk::{symbol_short, Env, IntoVal, String};

    // -----------------------------------------------------------------------
    // Setup helper
    // -----------------------------------------------------------------------

    fn setup() -> (Env, InsuranceClient<'static>, soroban_sdk::Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = soroban_sdk::Address::generate(&env);
        (env, client, owner)
    }

    /// Helper: create a policy with the given external_ref.
    fn create(
        env: &Env,
        client: &InsuranceClient,
        owner: &soroban_sdk::Address,
        ext_ref: Option<&str>,
    ) -> u32 {
        env.mock_all_auths();
        let ref_val = ext_ref.map(|s| String::from_str(env, s));
        client
            .create_policy(
                owner,
                &String::from_str(env, "Test Policy"),
                &CoverageType::Health,
                &100,
                &10_000,
                &ref_val,
            )
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // Task 8.2 — test_create_policy_indexes_external_ref
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_policy_indexes_external_ref() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));
        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, Some(id));
    }

    // -----------------------------------------------------------------------
    // Task 8.3 — test_create_policy_none_ref_no_index
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_policy_none_ref_no_index() {
        let (env, client, owner) = setup();
        create(&env, &client, &owner, None);
        let looked_up =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "anything"));
        assert_eq!(looked_up, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.4 — test_create_policy_duplicate_ref_rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_policy_duplicate_ref_rejected() {
        let (env, client, owner) = setup();
        create(&env, &client, &owner, Some("ref-A"));

        env.mock_all_auths();
        let result = client.try_create_policy(
            &owner,
            &String::from_str(&env, "Second Policy"),
            &CoverageType::Health,
            &100,
            &10_000,
            &Some(String::from_str(&env, "ref-A")),
        );
        assert_eq!(result, Err(Ok(InsuranceError::DuplicateExternalRef)));
    }

    // -----------------------------------------------------------------------
    // Task 8.5 — test_create_policy_invalid_ref_rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_policy_invalid_ref_rejected() {
        let (env, client, owner) = setup();

        // Empty string
        env.mock_all_auths();
        let result_empty = client.try_create_policy(
            &owner,
            &String::from_str(&env, "Policy"),
            &CoverageType::Health,
            &100,
            &10_000,
            &Some(String::from_str(&env, "")),
        );
        assert_eq!(result_empty, Err(Ok(InsuranceError::InvalidExternalRef)));

        // 129-byte string (exceeds 128-byte limit)
        let long_str: std::string::String = "x".repeat(129);
        env.mock_all_auths();
        let result_long = client.try_create_policy(
            &owner,
            &String::from_str(&env, "Policy"),
            &CoverageType::Health,
            &100,
            &10_000,
            &Some(String::from_str(&env, &long_str)),
        );
        assert_eq!(result_long, Err(Ok(InsuranceError::InvalidExternalRef)));
    }

    // -----------------------------------------------------------------------
    // Task 8.6 — test_deactivate_removes_ref_from_index
    // -----------------------------------------------------------------------

    #[test]
    fn test_deactivate_removes_ref_from_index() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        env.mock_all_auths();
        let result = client.deactivate_policy(&owner, &id);
        assert_eq!(result, true);

        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.7 — test_deactivate_none_ref_no_index_change
    // -----------------------------------------------------------------------

    #[test]
    fn test_deactivate_none_ref_no_index_change() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, None);

        // Should not panic
        env.mock_all_auths();
        let result = client.deactivate_policy(&owner, &id);
        assert_eq!(result, true);

        // Index should still be empty
        let looked_up =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "anything"));
        assert_eq!(looked_up, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.8 — test_deactivate_already_inactive_no_index_change
    // -----------------------------------------------------------------------

    #[test]
    fn test_deactivate_already_inactive_no_index_change() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        // First deactivation
        env.mock_all_auths();
        let first = client.deactivate_policy(&owner, &id);
        assert_eq!(first, true);

        // Second deactivation — policy is already inactive, returns false
        env.mock_all_auths();
        let second = client.deactivate_policy(&owner, &id);
        assert_eq!(second, false);

        // Index should still be empty (not re-added)
        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.9 — test_archive_removes_ref_from_index
    // -----------------------------------------------------------------------

    #[test]
    fn test_archive_removes_ref_from_index() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        env.mock_all_auths();
        let result = client.archive_policy(&owner, &id);
        assert_eq!(result, true);

        // Index entry removed
        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, None);

        // Policy itself removed
        let policy = client.get_policy(&id);
        assert_eq!(policy, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.10 — test_archive_none_ref_no_index_change
    // -----------------------------------------------------------------------

    #[test]
    fn test_archive_none_ref_no_index_change() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, None);

        // Should not panic
        env.mock_all_auths();
        let result = client.archive_policy(&owner, &id);
        assert_eq!(result, true);

        // Index should still be empty
        let looked_up =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "anything"));
        assert_eq!(looked_up, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.11 — test_reuse_after_archive
    // -----------------------------------------------------------------------

    #[test]
    fn test_reuse_after_archive() {
        let (env, client, owner) = setup();
        let id_a = create(&env, &client, &owner, Some("ref-A"));

        // Archive policy A
        env.mock_all_auths();
        client.archive_policy(&owner, &id_a);

        // Create policy B with the same ref
        let id_b = create(&env, &client, &owner, Some("ref-A"));
        assert_ne!(id_a, id_b);

        // Lookup should now return B's ID
        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, Some(id_b));
    }

    // -----------------------------------------------------------------------
    // Task 8.12 — test_set_external_ref_reindex
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_reindex() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        env.mock_all_auths();
        let result = client.set_external_ref(
            &owner,
            &id,
            &Some(String::from_str(&env, "ref-B")),
        );
        assert_eq!(result, true);

        // Old ref removed
        let old = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(old, None);

        // New ref indexed
        let new = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-B"));
        assert_eq!(new, Some(id));
    }

    // -----------------------------------------------------------------------
    // Task 8.13 — test_set_external_ref_to_none
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_to_none() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        env.mock_all_auths();
        let result = client.set_external_ref(&owner, &id, &None);
        assert_eq!(result, true);

        // Old ref removed
        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.14 — test_set_external_ref_duplicate_rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_duplicate_rejected() {
        let (env, client, owner) = setup();
        let id1 = create(&env, &client, &owner, Some("ref-A"));
        let _id2 = create(&env, &client, &owner, Some("ref-B"));

        // Try to set policy 1's ref to "ref-B" (already held by policy 2)
        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &owner,
            &id1,
            &Some(String::from_str(&env, "ref-B")),
        );
        assert_eq!(result, Err(Ok(InsuranceError::DuplicateExternalRef)));
    }

    // -----------------------------------------------------------------------
    // Task 8.15 — test_set_external_ref_invalid_rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_invalid_rejected() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        // Empty string
        env.mock_all_auths();
        let result_empty =
            client.try_set_external_ref(&owner, &id, &Some(String::from_str(&env, "")));
        assert_eq!(result_empty, Err(Ok(InsuranceError::InvalidExternalRef)));

        // 129-byte string
        let long_str: std::string::String = "y".repeat(129);
        env.mock_all_auths();
        let result_long = client.try_set_external_ref(
            &owner,
            &id,
            &Some(String::from_str(&env, &long_str)),
        );
        assert_eq!(result_long, Err(Ok(InsuranceError::InvalidExternalRef)));
    }

    // -----------------------------------------------------------------------
    // Task 8.16 — test_set_external_ref_idempotent
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_idempotent() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        // Capture event count before idempotent call
        let events_before = env.events().all().len();

        // Set the same ref again — should be idempotent
        env.mock_all_auths();
        let result = client.set_external_ref(
            &owner,
            &id,
            &Some(String::from_str(&env, "ref-A")),
        );
        assert_eq!(result, true);

        // No new event should have been emitted
        let events_after = env.events().all().len();
        assert_eq!(
            events_before, events_after,
            "idempotent set_external_ref must not emit an event"
        );

        // Index still correct
        let looked_up = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A"));
        assert_eq!(looked_up, Some(id));
    }

    // -----------------------------------------------------------------------
    // Task 8.17 — test_set_external_ref_emits_event
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_emits_event() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        env.mock_all_auths();
        client.set_external_ref(&owner, &id, &Some(String::from_str(&env, "ref-B")));

        let events = env.events().all();
        assert!(!events.is_empty(), "at least one event must be emitted");

        // Find the ext_upd event
        let expected_topic = symbol_short!("ext_upd");
        let found = events.iter().any(|e| {
            let topics = e.1;
            if topics.is_empty() {
                return false;
            }
            let t0 = soroban_sdk::Symbol::try_from_val(&env, &topics.get(0).unwrap());
            matches!(t0, Ok(s) if s == expected_topic)
        });
        assert!(found, "EVT_EXT_REF_UPDATED event must be emitted");

        // Decode the event payload and verify fields
        let evt = events
            .iter()
            .find(|e| {
                let topics = e.1;
                if topics.is_empty() {
                    return false;
                }
                let t0 = soroban_sdk::Symbol::try_from_val(&env, &topics.get(0).unwrap());
                matches!(t0, Ok(s) if s == expected_topic)
            })
            .unwrap();

        let payload: ExternalRefUpdatedEvent =
            soroban_sdk::FromVal::from_val(&env, &evt.2);
        assert_eq!(payload.policy_id, id);
        assert_eq!(
            payload.old_external_ref,
            Some(String::from_str(&env, "ref-A"))
        );
        assert_eq!(
            payload.new_external_ref,
            Some(String::from_str(&env, "ref-B"))
        );
    }

    // -----------------------------------------------------------------------
    // Task 8.18 — test_set_external_ref_sequential_abc
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_sequential_abc() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-A"));

        // A → B
        env.mock_all_auths();
        client.set_external_ref(&owner, &id, &Some(String::from_str(&env, "ref-B")));

        // B → C
        env.mock_all_auths();
        client.set_external_ref(&owner, &id, &Some(String::from_str(&env, "ref-C")));

        // Only C should be in the index
        assert_eq!(
            client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A")),
            None
        );
        assert_eq!(
            client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-B")),
            None
        );
        assert_eq!(
            client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-C")),
            Some(id)
        );
    }

    // -----------------------------------------------------------------------
    // Task 8.19 — test_lookup_active_policy
    // -----------------------------------------------------------------------

    #[test]
    fn test_lookup_active_policy() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-active"));

        let looked_up =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-active"));
        assert_eq!(looked_up, Some(id));

        // Cross-check with get_policy
        let policy = client.get_policy(&id).unwrap();
        assert_eq!(policy.id, id);
    }

    // -----------------------------------------------------------------------
    // Task 8.20 — test_lookup_unknown_ref_returns_none
    // -----------------------------------------------------------------------

    #[test]
    fn test_lookup_unknown_ref_returns_none() {
        let (env, client, _owner) = setup();
        let result =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "never-registered"));
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.21 — test_lookup_stability
    // -----------------------------------------------------------------------

    #[test]
    fn test_lookup_stability() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-stable"));

        let r1 = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-stable"));
        let r2 = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-stable"));
        let r3 = client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-stable"));

        assert_eq!(r1, Some(id));
        assert_eq!(r2, Some(id));
        assert_eq!(r3, Some(id));
    }

    // -----------------------------------------------------------------------
    // Task 8.22 — test_lookup_no_stale_after_deactivate
    // -----------------------------------------------------------------------

    #[test]
    fn test_lookup_no_stale_after_deactivate() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-deact"));

        env.mock_all_auths();
        client.deactivate_policy(&owner, &id);

        let result =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-deact"));
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.22 — test_lookup_no_stale_after_archive
    // -----------------------------------------------------------------------

    #[test]
    fn test_lookup_no_stale_after_archive() {
        let (env, client, owner) = setup();
        let id = create(&env, &client, &owner, Some("ref-arch"));

        env.mock_all_auths();
        client.archive_policy(&owner, &id);

        let result =
            client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-arch"));
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // Task 8.23 — proptest_round_trip
    //
    // Validates: Requirements R1.5, R6.9
    //
    // For any valid external_ref string (1–128 ASCII alphanumeric bytes),
    // create_policy followed by get_policy_id_by_external_ref returns the
    // correct policy ID.
    // -----------------------------------------------------------------------

    proptest! {
        /// **Validates: Requirements R1.5, R6.9**
        ///
        /// For any valid `external_ref` string (1–128 ASCII bytes),
        /// `create_policy` followed by `get_policy_id_by_external_ref`
        /// returns the correct policy ID (round-trip property).
        #[test]
        fn proptest_round_trip(
            ref_str in prop::string::string_regex("[a-zA-Z0-9]{1,128}").unwrap()
        ) {
            let env = Env::default();
            let contract_id = env.register_contract(None, Insurance);
            let client = InsuranceClient::new(&env, &contract_id);
            let owner = soroban_sdk::Address::generate(&env);

            env.mock_all_auths();
            let id = client
                .create_policy(
                    &owner,
                    &String::from_str(&env, "Prop Policy"),
                    &CoverageType::Health,
                    &100,
                    &10_000,
                    &Some(String::from_str(&env, &ref_str)),
                )
                .unwrap();

            let looked_up =
                client.get_policy_id_by_external_ref(&String::from_str(&env, &ref_str));
            prop_assert_eq!(looked_up, Some(id));
        }
    }
}
