# Integrating Sidera resolution into a wallet or payment app

Forward resolution turns `alice.sid` into a payment destination. This guide
covers what an integration **must** do; `docs/resolver-interface.md` specifies
the contract interface itself, and the testnet deployment used below is listed
in the [README](../README.md#testnet-deployment-proof).

## The five rules

1. **Resolve against the official registry contract only.** Pin the registry
   contract ID in your client (testnet ID in the README; a SEP-style
   announcement channel will follow for mainnet). Never accept a registry
   address from user input.
2. **Attach the memo when one is present.** If `resolve` returns
   `memo: Some(...)`, your payment MUST include that memo, or it MUST abort
   with an explicit warning to the user. A memo-hinted payment sent without
   its memo is the #1 way funds are lost on Stellar — silently dropping it is
   the failure mode Sidera exists to prevent.
3. **Display the resolved address next to the name.** Users should always be
   able to see the full `G...`/`M...` address they are about to pay.
4. **Handle muxed (M-) addresses natively.** Sidera stores whatever address
   the owner registered, including muxed accounts. Pass the resolved address
   through to your payment flow without stripping or rewriting it — the
   56-character string your user no longer has to paste still has to arrive
   at the network intact.
5. **Never pay an expired name.** `resolve` fails with `NameExpired` (SDK:
   `NameExpiredError`) once a name's registration runs out. Treat it like an
   unknown name: stop, and never fall back to a cached address. You can show
   the user when it expired with `expiresAt(name)`.

## Calling `resolve`

`resolve` is a read-only invocation, so it costs no fee and can be simulated
against any Soroban RPC endpoint. With `@stellar/stellar-sdk` (v14),
`contract.Client.from` fetches the deployed contract's spec and exposes the
entrypoints directly. (`new contract.Client(...)` needs the spec as its first
argument, so don't call it with options alone.) `resolve` returns a Soroban
`Result`: `unwrap()` gives the record, or throws for an unknown or expired
name.

```typescript
import { contract } from "@stellar/stellar-sdk";

const REGISTRY_ID = "CD4YE4ERU37U5KQU3QCJJGIKUSES6QXA6WNLIMORRBY62RD37SC6FGUT";
const RPC_URL = "https://soroban-testnet.stellar.org";
const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";

// Read-only calls need no signer.
const registry = await contract.Client.from({
  contractId: REGISTRY_ID,
  rpcUrl: RPC_URL,
  networkPassphrase: TESTNET_PASSPHRASE,
});

export async function resolveSid(name: string): Promise<{
  address: string;
  memo: string | null;
  memoType: "Inferred" | "Id" | "Text";
}> {
  const { result } = await registry.resolve({ name });
  const record = result.unwrap(); // throws for an unknown or expired name
  return {
    address: record.address.toString(),
    memo: record.memo ?? null,
    memoType: record.memo_type.tag, // see "Memo types" below
  };
}
```

The same call from the CLI (useful for smoke-testing a deployment):

```bash
stellar contract invoke \
  --id CD4YE4ERU37U5KQU3QCJJGIKUSES6QXA6WNLIMORRBY62RD37SC6FGUT \
  --network testnet --source <any-identity> --send no \
  -- resolve --name alice-pay
```

## Installing the SDK in your own project

`@sidera-protocol/sdk` is not on the npm registry yet, so `npm install
@sidera-protocol/sdk` fails with a 404. Each SDK release is attached to a
[GitHub release](https://github.com/Sidera-Protocol/sidera-protocol/releases)
instead; install it by URL:

```bash
echo "allow-remote=root" >> .npmrc   # npm 12+ refuses package URLs without it
npm install https://github.com/Sidera-Protocol/sidera-protocol/releases/download/sdk-v0.1.0/sidera-protocol-sdk-0.1.0.tgz
```

npm records the URL in your `package.json` and installs
`@stellar/stellar-sdk` with it. Keep the `.npmrc` line in your project:
npm 12 and later refuse dependencies given as a URL unless `allow-remote`
allows them, so later installs of your project need it too. Older npm
versions don't need the line. To build the package yourself instead, run
`npm ci && npm run build -w @sidera-protocol/sdk` in this repository, then
`npm pack` in `packages/sdk`, and install the `.tgz` it writes.

Your app then imports it as usual (the package is ES modules only):

```typescript
import { SideraClient } from "@sidera-protocol/sdk";

const sidera = new SideraClient({
  contractId: "CD4YE4ERU37U5KQU3QCJJGIKUSES6QXA6WNLIMORRBY62RD37SC6FGUT",
  network: "testnet",
});
console.log(await sidera.resolve("alice-pay"));
// { name: "alice-pay", fullName: "alice-pay.sid", address: "GCSR…6MMO",
//   memo: { type: "id", value: "1029384756" } }
```

## Paying a name

With the SDK, `buildPayment` builds the payment from the resolution, so the
memo is attached for you:

```typescript
import { Horizon, Networks } from "@stellar/stellar-sdk";
import { SideraClient, buildPayment } from "@sidera-protocol/sdk";

const sidera = new SideraClient({ contractId: REGISTRY_ID, network: "testnet" });
const horizon = new Horizon.Server("https://horizon-testnet.stellar.org");

const to = await sidera.resolve("exchange-user.sid"); // throws if unknown or expired
// Show to.address and to.memo to the user and wait for confirmation, then:
const tx = buildPayment({
  source: await horizon.loadAccount(payer),
  to,
  amount: "10",
  networkPassphrase: Networks.TESTNET,
});
// Sign tx with the user's wallet, then horizon.submitTransaction(signed).
```

`scripts/pay-by-name-demo.mjs` runs this on testnet and checks on Horizon
that the payment arrived with the right memo
([receipt](pay-by-name-receipt.json)); the dashboard's Send page does the
same with Freighter.

## Subnames

`pay.alice.sid` is resolved exactly like `alice.sid`: pass it to `resolve`
(the SDK strips `.sid`). The rules above apply unchanged; in particular,
attach the subname's own memo, not the parent's. An exchange can give every
customer a name such as `alice.exchange.sid` whose memo is that customer's
deposit number. If the exchange needs that number as `MEMO_TEXT` rather than
`MEMO_ID`, it states so once per name with `set_memo_type` (SDK:
`setMemoType("alice.exchange", "text", owner)`); see
[Memo types](#memo-types).

## Memo types

Each record carries `memo_type`: `Inferred`, `Id` or `Text`.

- `Id` / `Text`: the owner stated how the memo must be sent. Send it exactly
  that way; a numeric memo marked `Text` goes out as `MEMO_TEXT`.
- `Inferred`: nothing stated. Send digits with no leading zero that fit a u64
  as `MEMO_ID`, and anything else as `MEMO_TEXT`, byte for byte.

The SDK applies this for you: `resolve(...).memo` has the right `type`, and
`stated: true` when the owner stated it. Records written before memo types
existed read as `Inferred`. Changing a memo with `set_address` or
`set_subname` clears its stated type, because it was stated for the old memo.

## Watching for changes

The registry emits one event per state change, with the name as the second
topic, so indexers can subscribe once and filter by name or by event:

| Event | Topics | Data | Meaning |
|---|---|---|---|
| `registered` | `("registered", name)` | `(owner, address, memo)` | A new name exists |
| `address_set` | `("address_set", name)` | `(caller, address, memo)` | Payment record updated |
| `subname_set` | `("subname_set", parent)` | `(label, caller, record)` | Subname `label.parent` set, or deleted (`record` is `None`) |
| `record_set` | `("record_set", name)` | `(caller, key, value)` | Profile record set, or deleted (`value` is `None`) |
| `memo_type_set` | `("memo_type_set", name)` | `(caller, memo_type)` | A name's memo type stated or cleared (for a subname, `subname_set` carries the updated record) |
| `transferred` | `("transferred", name)` | `(caller, new_owner)` | Ownership moved |
| `primary_name_set` | `("primary_name_set", address)` | `(name)` | Preferred reverse name changed |

A wallet that caches resolutions should re-resolve when it observes
`address_set` or `transferred` for a name it holds, and `subname_set` for
the parent of a subname it holds. A name's subnames and profile also end
when it is registered again by someone else (a `registered` event for a name
you had cached).

## Cache invalidation for cached resolution results

If your wallet caches `resolve` results, invalidate or refresh the cache
whenever it observes any of these registry events for a name or address it
cares about:

- `address_set(name)` — the payment destination or memo hint for a name has
  changed.
- `transferred(name)` — ownership of a name changed; the applicable wallet
  should re-resolve because the new owner may publish a different memo hint.
- `registered(name)` — a new name exists; the wallet should treat the first
  cached lookup as a cache miss and resolve against the contract.

If you also cache reverse-resolution results produced by `primary_name`,
invalidate them when the address owner calls `set_primary_name` or when the
address record changes in a way your wallet tracks. The SDK exposes both
`primaryName(address)` and `setPrimaryName(address, name)`; integrations
should use those methods rather than duplicating contract invocation logic.

## What Sidera does not do yet

An indexer is a future tranche — see
[KNOWN-LIMITATIONS.md](KNOWN-LIMITATIONS.md) and the roadmap in the
[README](../README.md#status). Paid registration, renewal and expiry, and
verified reverse resolution, are implemented across the registry, SDK and
dashboard. The registry is deployed on **testnet only**;
do not point production payment flows at it.
