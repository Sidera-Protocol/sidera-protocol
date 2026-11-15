// Live proof of stated memo types on a deployed Sidera registry.
//
//  1. Names written before memo types existed (`exchange-user`, memo
//     48151623) read back as `Inferred`, so wallets keep sending MEMO_ID.
//  2. An exchange gives a customer `text.demo-exchange` with the numeric
//     memo 2002 and states it must be sent as MEMO_TEXT.
//  3. Bob pays 1 XLM to `text.demo-exchange.sid`; Horizon shows the payment
//     carried memo 2002 as MEMO_TEXT, not MEMO_ID.
//  4. Changing the subname's memo clears the stated type.
//  5. A MEMO_ID claim for a memo that isn't a u64, and a stranger's claim,
//     are both refused.
//
// Needs funded Stellar CLI identities sidera-exchange and sidera-bob, and a
// built SDK:
//
//   CONTRACT=C... node scripts/memo-type-demo.mjs receipt.json
//
// Each transaction is signed by the Stellar CLI, so no secret key is read here.

import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { Horizon, Networks, TransactionBuilder } from "@stellar/stellar-sdk";
import { SideraClient, buildPayment } from "../packages/sdk/dist/index.js";

const CONTRACT = process.env.CONTRACT;
const PARENT = process.env.PARENT ?? "demo-exchange";
const LEGACY = process.env.LEGACY ?? "exchange-user";
const horizon = new Horizon.Server("https://horizon-testnet.stellar.org");

const addr = (k) => execFileSync("stellar", ["keys", "address", k]).toString().trim();
const sign = (identity, txXdr) =>
  execFileSync("stellar", ["tx", "sign", "--sign-with-key", identity, "--network", "testnet"], { input: txXdr }).toString().trim();
const as = (identity) =>
  new SideraClient({ contractId: CONTRACT, network: "testnet", signer: { signTx: async (x) => sign(identity, x) } });

const EXCHANGE = addr("sidera-exchange"), BOB = addr("sidera-bob");
const exchange = as("sidera-exchange"), bob = as("sidera-bob");

const receipt = { network: "testnet", contractId: CONTRACT, sdk: "@sidera-protocol/sdk (packages/sdk)", steps: [] };
let failed = false;
const step = (what, data) => {
  if (data.ok === false) failed = true;
  receipt.steps.push({ what, ...data });
  console.log(data.ok === false ? "✗" : "✓", what, JSON.stringify(data));
};

const legacy = await bob.resolve(LEGACY);
step("a name written before memo types reads as inferred", {
  name: `${LEGACY}.sid`,
  memo: legacy.memo,
  ok: legacy.memo?.type === "id" && legacy.memo?.stated !== true,
});

const SUB = `text.${PARENT}`;
const h1 = await exchange.setSubname({ parent: PARENT, label: "text", address: EXCHANGE, memo: "2002" }, EXCHANGE);
const h2 = await exchange.setMemoType(SUB, "text", EXCHANGE);
const r1 = await bob.resolve(SUB);
step("the exchange states that a numeric memo is MEMO_TEXT", {
  name: `${SUB}.sid`,
  hashes: [h1, h2],
  memo: r1.memo,
  ok: r1.memo?.type === "text" && r1.memo?.value === "2002" && r1.memo?.stated === true,
});

const tx = buildPayment({ source: await horizon.loadAccount(BOB), to: r1, amount: "1", networkPassphrase: Networks.TESTNET });
const signed = TransactionBuilder.fromXDR(sign("sidera-bob", tx.toXDR()), Networks.TESTNET);
const sent = await horizon.submitTransaction(signed);
const onChain = await horizon.transactions().transaction(sent.hash).call();
step("Bob pays by name; Horizon shows memo 2002 sent as MEMO_TEXT", {
  hash: sent.hash,
  memo_type: onChain.memo_type,
  memo: onChain.memo,
  ok: onChain.memo_type === "text" && onChain.memo === "2002",
});

const h3 = await exchange.setSubname({ parent: PARENT, label: "text", address: EXCHANGE, memo: "2003" }, EXCHANGE);
const r2 = await bob.resolve(SUB);
step("changing the memo clears the stated type", {
  hash: h3,
  memo: r2.memo,
  ok: r2.memo?.type === "id" && r2.memo?.stated !== true,
});

for (const [what, run, code] of [
  ["MEMO_ID for a memo that is not a u64 is refused", async () => {
    await exchange.setSubname({ parent: PARENT, label: "text", address: EXCHANGE, memo: "ref-7" }, EXCHANGE);
    await exchange.setMemoType(SUB, "id", EXCHANGE);
  }, "InvalidMemo"],
  ["a stranger can't state a memo type", () => bob.setMemoType(SUB, "text", BOB), "Unauthorized"],
]) {
  try {
    await run();
    step(`UNEXPECTED: ${what.replace(" is refused", " was accepted")}`, { ok: false });
  } catch (e) {
    step(what, { refusedWith: e.code ?? e.constructor.name, ok: e.code === code });
  }
}

const out = process.argv[2];
if (out) writeFileSync(out, JSON.stringify(receipt, null, 2) + "\n");
if (failed) process.exit(1);
