//! Stated memo types (`set_memo_type`).
//!
//! The registry stores memos as text. Without a stated type, wallets send
//! digits that fit a u64 as `MEMO_ID`, so an exchange that needs a numeric
//! memo as `MEMO_TEXT` had no way to say so.

use super::*;
use crate::testutil::{deploy, free};
use soroban_sdk::testutils::{Address as _, Ledger};

fn s(env: &Env, raw: &str) -> String {
    String::from_str(env, raw)
}

fn setup() -> (Env, Address, SideraRegistryClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = deploy(&env, free()).id;
    let c = SideraRegistryClient::new(&env, &id);
    let owner = Address::generate(&env);
    c.register(
        &s(&env, "exchange"),
        &owner,
        &owner,
        &Some(s(&env, "1001")),
        &1,
    );
    (env, id, c, owner)
}

#[test]
fn a_new_record_has_no_stated_type() {
    let (env, _, c, _) = setup();
    assert_eq!(
        c.resolve(&s(&env, "exchange")).memo_type,
        MemoType::Inferred
    );
}

#[test]
fn the_owner_can_state_that_a_numeric_memo_is_text() {
    let (env, _, c, owner) = setup();
    c.set_memo_type(&s(&env, "exchange"), &owner, &MemoType::Text);

    let r = c.resolve(&s(&env, "exchange"));
    assert_eq!(r.memo, Some(s(&env, "1001")));
    assert_eq!(r.memo_type, MemoType::Text);

    // And clear it again.
    c.set_memo_type(&s(&env, "exchange"), &owner, &MemoType::Inferred);
    assert_eq!(
        c.resolve(&s(&env, "exchange")).memo_type,
        MemoType::Inferred
    );
}

#[test]
fn id_needs_a_u64_in_canonical_decimal() {
    let (env, _, c, owner) = setup();
    let name = s(&env, "exchange");
    for (memo, ok) in [
        ("0", true),
        ("1001", true),
        ("18446744073709551615", true),  // u64::MAX
        ("18446744073709551616", false), // u64::MAX + 1
        ("007", false),
        ("12a", false),
        ("-1", false),
        ("ref-42", false),
    ] {
        c.set_address(&name, &owner, &owner, &Some(s(&env, memo)));
        let result = c.try_set_memo_type(&name, &owner, &MemoType::Id);
        if ok {
            assert_eq!(result, Ok(Ok(())), "{memo} should be accepted as MEMO_ID");
            assert_eq!(c.resolve(&name).memo_type, MemoType::Id);
        } else {
            assert_eq!(
                result,
                Err(Ok(RegistryError::InvalidMemo)),
                "{memo} is not a u64"
            );
            assert_eq!(c.resolve(&name).memo_type, MemoType::Inferred);
        }
        // Text is always allowed for a memo that exists.
        c.set_memo_type(&name, &owner, &MemoType::Text);
    }
}

#[test]
fn a_record_without_a_memo_cannot_be_typed() {
    let (env, _, c, owner) = setup();
    let name = s(&env, "exchange");
    c.set_address(&name, &owner, &owner, &None);
    assert_eq!(
        c.try_set_memo_type(&name, &owner, &MemoType::Text),
        Err(Ok(RegistryError::InvalidMemo))
    );
    assert_eq!(
        c.try_set_memo_type(&name, &owner, &MemoType::Id),
        Err(Ok(RegistryError::InvalidMemo))
    );
    c.set_memo_type(&name, &owner, &MemoType::Inferred);
}

#[test]
fn only_the_owner_of_an_active_name_can_state_a_type() {
    let (env, _, c, _) = setup();
    let stranger = Address::generate(&env);
    assert_eq!(
        c.try_set_memo_type(&s(&env, "exchange"), &stranger, &MemoType::Text),
        Err(Ok(RegistryError::Unauthorized))
    );
    assert_eq!(
        c.try_set_memo_type(&s(&env, "missing"), &stranger, &MemoType::Text),
        Err(Ok(RegistryError::NotFound))
    );

    let (env, _, c, owner) = setup();
    env.ledger().with_mut(|l| l.timestamp += YEAR_SECONDS + 1);
    assert_eq!(
        c.try_set_memo_type(&s(&env, "exchange"), &owner, &MemoType::Text),
        Err(Ok(RegistryError::NameExpired))
    );
}

#[test]
fn changing_the_memo_clears_its_stated_type_and_keeping_it_does_not() {
    let (env, _, c, owner) = setup();
    let name = s(&env, "exchange");
    c.set_memo_type(&name, &owner, &MemoType::Text);

    // A new address with the same memo keeps the type.
    let other = Address::generate(&env);
    c.set_address(&name, &owner, &other, &Some(s(&env, "1001")));
    assert_eq!(c.resolve(&name).memo_type, MemoType::Text);

    // A different memo clears it: the type was stated for the old memo.
    c.set_address(&name, &owner, &other, &Some(s(&env, "1002")));
    assert_eq!(c.resolve(&name).memo_type, MemoType::Inferred);
}

#[test]
fn a_subname_can_have_its_own_memo_type() {
    let (env, _, c, owner) = setup();
    let deposit = Address::generate(&env);
    let parent = s(&env, "exchange");
    c.set_subname(
        &parent,
        &s(&env, "alice"),
        &owner,
        &deposit,
        &Some(s(&env, "1001")),
    );
    c.set_subname(
        &parent,
        &s(&env, "bob"),
        &owner,
        &deposit,
        &Some(s(&env, "1002")),
    );

    c.set_memo_type(&s(&env, "alice.exchange"), &owner, &MemoType::Text);

    assert_eq!(
        c.resolve(&s(&env, "alice.exchange")).memo_type,
        MemoType::Text
    );
    assert_eq!(
        c.resolve(&s(&env, "bob.exchange")).memo_type,
        MemoType::Inferred
    );
    // The parent's own record is untouched.
    assert_eq!(c.resolve(&parent).memo_type, MemoType::Inferred);
    let subs = c.subnames(&parent);
    assert_eq!(
        subs.get(s(&env, "alice")).unwrap().memo_type,
        MemoType::Text
    );

    // Re-saving the subname with the same memo keeps the type; a new memo clears it.
    c.set_subname(
        &parent,
        &s(&env, "alice"),
        &owner,
        &deposit,
        &Some(s(&env, "1001")),
    );
    assert_eq!(
        c.resolve(&s(&env, "alice.exchange")).memo_type,
        MemoType::Text
    );
    c.set_subname(
        &parent,
        &s(&env, "alice"),
        &owner,
        &deposit,
        &Some(s(&env, "9")),
    );
    assert_eq!(
        c.resolve(&s(&env, "alice.exchange")).memo_type,
        MemoType::Inferred
    );

    let stranger = Address::generate(&env);
    assert_eq!(
        c.try_set_memo_type(&s(&env, "bob.exchange"), &stranger, &MemoType::Text),
        Err(Ok(RegistryError::Unauthorized))
    );
    assert_eq!(
        c.try_set_memo_type(&s(&env, "carol.exchange"), &owner, &MemoType::Text),
        Err(Ok(RegistryError::NotFound))
    );
}

/// The record shape stored before memo types existed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct PreMemoTypeResolution {
    address: MuxedAddress,
    memo: Option<String>,
}

#[test]
fn records_stored_before_memo_types_can_be_typed_after_the_upgrade() {
    let (env, id, c, owner) = setup();
    c.set_subname(
        &s(&env, "exchange"),
        &s(&env, "alice"),
        &owner,
        &owner,
        &None,
    );
    let memo = Some(s(&env, "1001"));

    env.as_contract(&id, || {
        let storage = env.storage().persistent();
        storage.set(
            &DataKey::Name(s(&env, "exchange")),
            &PreMemoTypeResolution {
                address: owner.clone().into(),
                memo: memo.clone(),
            },
        );
        let epoch: u32 = storage.get(&DataKey::Epoch(s(&env, "exchange"))).unwrap();
        let mut subs: Map<String, PreMemoTypeResolution> = Map::new(&env);
        subs.set(
            s(&env, "alice"),
            PreMemoTypeResolution {
                address: owner.clone().into(),
                memo: memo.clone(),
            },
        );
        storage.set(&DataKey::Subnames(s(&env, "exchange"), epoch), &subs);
    });

    // Old records read as "not stated"...
    assert_eq!(
        c.resolve(&s(&env, "exchange")).memo_type,
        MemoType::Inferred
    );
    assert_eq!(
        c.resolve(&s(&env, "alice.exchange")).memo_type,
        MemoType::Inferred
    );

    // ...and can be typed, which rewrites them in the new shape.
    c.set_memo_type(&s(&env, "exchange"), &owner, &MemoType::Text);
    c.set_memo_type(&s(&env, "alice.exchange"), &owner, &MemoType::Text);
    assert_eq!(c.resolve(&s(&env, "exchange")).memo_type, MemoType::Text);
    assert_eq!(
        c.resolve(&s(&env, "alice.exchange")).memo_type,
        MemoType::Text
    );
    assert_eq!(c.resolve(&s(&env, "exchange")).memo, memo);
}
