# Development and release checks

## Contract checks

From the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo build --locked --release --target wasm32v1-none -p sidera-registry
```

## Full suite verification

Run the contract, SDK and dashboard checks with one command from the
repository root:

```bash
npm ci
bash scripts/verify-suite.sh
```

It stops at the first failing check.

The contract build must use the pinned lockfile and the `wasm32v1-none`
target. Changes to storage, authorization, name validation, TTL behavior, or
resolver output should include focused tests.

## SDK checks

From the repository root:

```bash
npm ci --ignore-scripts
npm run lint -w @sidera-protocol/sdk
npm test -w @sidera-protocol/sdk
npm run build -w @sidera-protocol/sdk
```

The SDK should be checked whenever the contract interface, error codes,
resolution shape, memo rules, or transaction parameters change.

## Dashboard checks

From the repository root (the dashboard uses the SDK from `packages/sdk`, so
build the SDK first):

```bash
npm ci --ignore-scripts
npm run build -w @sidera-protocol/sdk
npm run lint -w @sidera-protocol/app
npm run build -w @sidera-protocol/app
```

For wallet or network changes, also verify the testnet flow manually with a
non-production account. Keep `NEXT_PUBLIC_SIDERIA_CONTRACT_ID` and
`NEXT_PUBLIC_SIDERIA_NETWORK` aligned with the documented deployment.

## Release checklist

- Confirm the contract ID and network in the relevant README files.
- Rebuild the contract from a clean checkout and record the WASM hash.
- Run contract, SDK, and dashboard checks.
- Review memo-hint handling and authorization behavior.
- Update known limitations and migration notes when behavior changes.
- Announce mainnet separately; never infer mainnet readiness from testnet CI.
