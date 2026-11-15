//! Muxed (M-) addresses as payment destinations.
//!
//! A muxed address is a base account plus a u64 id, used by exchanges and
//! custodians to give each customer a virtual account. A name must resolve to
//! the exact muxed address it was set to, id included: dropping the id sends
//! the payment to the base account with nothing to say whose it is.

use super::*;
use crate::testutil::{deploy, free};
use soroban_sdk::testutils::{Address as _, MuxedAddress as _};

fn s(env: &Env, raw: &str) -> String {
    String::from_str(env, raw)
}

fn setup() -> (Env, Address, SideraRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let id = deploy(&env, free()).id;
    let client = SideraRegistryClient::new(&env, &id);
    (env, id, client)
}

#[test]
fn a_name_resolves_to_the_exact_muxed_address_it_was_registered_with() {
    let (env, _, c) = setup();
    let deposit = MuxedAddress::generate(&env);
    let owner = Address::generate(&env);

    c.register(&s(&env, "exchange-user"), &owner, &deposit, &None, &1);

    let r = c.resolve(&s(&env, "exchange-user"));
    assert_eq!(r.address, deposit);
    assert_eq!(r.address.id(), deposit.id());
    assert!(r.address.id().is_some());
    assert_eq!(r.address.to_strkey(), deposit.to_strkey());
}

#[test]
fn set_address_moves_between_muxed_and_plain_addresses() {
    let (env, _, c) = setup();
    let owner = Address::generate(&env);
    let base = MuxedAddress::generate(&env);
    let customer_7 = MuxedAddress::new(base.clone(), 7);
    let customer_8 = MuxedAddress::new(base.clone(), 8);
    c.register(&s(&env, "alice"), &owner, &owner, &None, &1);

    c.set_address(&s(&env, "alice"), &owner, &customer_7, &None);
    assert_eq!(c.resolve(&s(&env, "alice")).address, customer_7);

    // Same base account, different id: a different destination.
    c.set_address(&s(&env, "alice"), &owner, &customer_8, &None);
    let r = c.resolve(&s(&env, "alice"));
    assert_eq!(r.address, customer_8);
    assert_eq!(r.address.id(), Some(8));
    assert_ne!(r.address, customer_7);

    // And back to a plain address, which has no id.
    c.set_address(&s(&env, "alice"), &owner, &owner, &None);
    let r = c.resolve(&s(&env, "alice"));
    assert_eq!(r.address, MuxedAddress::from(&owner));
    assert_eq!(r.address.id(), None);
}

#[test]
fn a_muxed_destination_keeps_its_memo() {
    let (env, _, c) = setup();
    let owner = Address::generate(&env);
    let deposit = MuxedAddress::generate(&env);
    let memo = Some(s(&env, "ref-42"));
    c.register(&s(&env, "alice"), &owner, &deposit, &memo, &1);

    assert_eq!(
        c.resolve(&s(&env, "alice")),
        Resolution {
            address: deposit,
            memo,
            memo_type: MemoType::Inferred,
        }
    );
}

#[test]
fn a_subname_resolves_to_a_muxed_address_and_lists_it() {
    let (env, _, c) = setup();
    let exchange = Address::generate(&env);
    c.register(&s(&env, "exchange"), &exchange, &exchange, &None, &1);
    let base = MuxedAddress::generate(&env);
    let alice_deposit = MuxedAddress::new(base.clone(), 1001);
    let bob_deposit = MuxedAddress::new(base, 1002);

    c.set_subname(
        &s(&env, "exchange"),
        &s(&env, "alice"),
        &exchange,
        &alice_deposit,
        &None,
    );
    c.set_subname(
        &s(&env, "exchange"),
        &s(&env, "bob"),
        &exchange,
        &bob_deposit,
        &None,
    );

    assert_eq!(c.resolve(&s(&env, "alice.exchange")).address, alice_deposit);
    assert_eq!(c.resolve(&s(&env, "bob.exchange")).address, bob_deposit);
    let subs = c.subnames(&s(&env, "exchange"));
    assert_eq!(subs.get(s(&env, "alice")).unwrap().address.id(), Some(1001));
    assert_eq!(subs.get(s(&env, "bob")).unwrap().address.id(), Some(1002));
}

#[test]
fn a_name_pointing_to_a_muxed_address_is_not_its_base_accounts_primary_name() {
    let (env, _, c) = setup();
    let base = MuxedAddress::generate(&env);
    let owner = base.address();
    let muxed = MuxedAddress::new(base, 5);
    c.register(&s(&env, "alice"), &owner, &muxed, &None, &1);

    // The name pays a virtual account of `owner`, not `owner` itself.
    assert_eq!(
        c.try_set_primary_name(&owner, &s(&env, "alice")),
        Err(Ok(RegistryError::Unauthorized))
    );

    // Pointing it at the plain base account makes it eligible.
    c.set_address(&s(&env, "alice"), &owner, &owner, &None);
    c.set_primary_name(&owner, &s(&env, "alice"));
    assert_eq!(c.primary_name(&owner), s(&env, "alice"));

    // Switching back to the muxed address stops the reverse lookup.
    c.set_address(&s(&env, "alice"), &owner, &muxed, &None);
    assert!(c.try_primary_name(&owner).is_err());
}

/// The record shape stored before muxed support: a plain `Address`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct LegacyResolution {
    address: Address,
    memo: Option<String>,
}

#[test]
fn records_written_before_muxed_support_still_resolve_after_the_upgrade() {
    let (env, id, c) = setup();
    let owner = Address::generate(&env);
    let dest = Address::generate(&env);
    let exchange = Address::generate(&env);
    c.register(&s(&env, "alice"), &owner, &owner, &None, &1);
    c.set_subname(&s(&env, "alice"), &s(&env, "pay"), &owner, &owner, &None);

    // Overwrite both records with the old shape, as the deployed contract
    // stored them before this change.
    env.as_contract(&id, || {
        let storage = env.storage().persistent();
        let memo = Some(s(&env, "48151623"));
        storage.set(
            &DataKey::Name(s(&env, "alice")),
            &LegacyResolution {
                address: dest.clone(),
                memo: memo.clone(),
            },
        );
        let epoch: u32 = storage.get(&DataKey::Epoch(s(&env, "alice"))).unwrap();
        let mut subs: Map<String, LegacyResolution> = Map::new(&env);
        subs.set(
            s(&env, "pay"),
            LegacyResolution {
                address: exchange.clone(),
                memo,
            },
        );
        storage.set(&DataKey::Subnames(s(&env, "alice"), epoch), &subs);
    });

    let r = c.resolve(&s(&env, "alice"));
    assert_eq!(r.address, MuxedAddress::from(&dest));
    assert_eq!(r.address.id(), None);
    assert_eq!(r.memo, Some(s(&env, "48151623")));
    assert_eq!(
        c.resolve(&s(&env, "pay.alice")).address,
        MuxedAddress::from(&exchange)
    );
    assert_eq!(c.subnames(&s(&env, "alice")).len(), 1);
}
