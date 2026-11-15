// Live proof that muxed (M-) addresses round-trip through a deployed Sidera
// registry, id included, and that a payment to the name reaches that id.
//
//  1. An exchange registers `muxed-demo` paying a muxed address of its
//     account (one customer's virtual account).
//  2. `resolve` returns that exact M-address.
//  3. Bob pays 1 XLM to `muxed-demo.sid`; Horizon shows the payment went to
//     the same M-address and mux id.
//  4. The exchange repoints the name to another customer's id, and gives a
//     third customer a muxed subname; both resolve exactly.
//  5. The exchange's account can't claim `muxed-demo` as its primary name:
//     the name pays a virtual account, not the exchange itself.
//
// Needs funded Stellar CLI identities sidera-exchange and sidera-bob, and a
// built SDK:
//
//   CONTRACT=C... node scripts/muxed-demo.mjs receipt.json
//
// Each transaction is signed by the Stellar CLI, so no secret key is read here.

import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { Account, Horizon, MuxedAccount, Networks, TransactionBuilder } from "@stellar/stellar-sdk";
import { SideraClient, buildPayment } from "../packages/sdk/dist/index.js";

const CONTRACT = process.env.CONTRACT;
const NAME = process.env.NAME ?? "muxed-demo";
const PARENT = process.env.PARENT ?? "demo-exchange";
const horizon = new Horizon.Server("https://horizon-testnet.stellar.org");

const addr = (k) => execFileSync("stellar", ["keys", "address", k]).toString().trim();
const sign = (identity, txXdr) =>
  execFileSync("stellar", ["tx", "sign", "--sign-with-key", identity, "--network", "testnet"], { input: txXdr }).toString().trim();
const as = (identity) =>
  new SideraClient({ contractId: CONTRACT, network: "testnet", signer: { signTx: async (x) => sign(identity, x) } });
const muxed = (g, id) => new MuxedAccount(new Account(g, "0"), id).accountId();

const EXCHANGE = addr("sidera-exchange"), BOB = addr("sidera-bob");
const exchange = as("sidera-exchange"), bob = as("sidera-bob");
const CUSTOMER_A = muxed(EXCHANGE, "4242424242");
const CUSTOMER_B = muxed(EXCHANGE, "4242424243");
const CUSTOMER_C = muxed(EXCHANGE, "4242424244");

const receipt = { network: "testnet", contractId: CONTRACT, sdk: "@sidera-protocol/sdk (packages/sdk)", steps: [] };
let failed = false;
const step = (what, data) => {
  if (data.ok === false) failed = true;
  receipt.steps.push({ what, ...data });
  console.log(data.ok === false ? "✗" : "✓", what, JSON.stringify(data));
};

const reg = await exchange.register({ name: NAME, owner: EXCHANGE, address: CUSTOMER_A, years: 1 });
step("exchange registers a name paying a customer's muxed address", { name: `${NAME}.sid`, address: CUSTOMER_A, hash: reg.hash });

const r1 = await bob.resolve(NAME);
step("resolve returns the exact M-address", { resolved: r1.address, ok: r1.address === CUSTOMER_A });

const tx = buildPayment({ source: await horizon.loadAccount(BOB), to: r1, amount: "1", networkPassphrase: Networks.TESTNET });
const signed = TransactionBuilder.fromXDR(sign("sidera-bob", tx.toXDR()), Networks.TESTNET);
const sent = await horizon.submitTransaction(signed);
const ops = await horizon.operations().forTransaction(sent.hash).call();
const pay = ops.records[0];
step("Bob pays 1 XLM to the name; Horizon shows it reached the muxed id", {
  hash: sent.hash,
  to: pay.to,
  to_muxed: pay.to_muxed,
  to_muxed_id: pay.to_muxed_id,
  amount: pay.amount,
  ok: pay.to_muxed === CUSTOMER_A && pay.to_muxed_id === "4242424242" && pay.to === EXCHANGE,
});

const h2 = await exchange.setAddress({ name: NAME, address: CUSTOMER_B }, EXCHANGE);
const r2 = await bob.resolve(NAME);
step("set_address to another customer's id resolves exactly", { hash: h2, resolved: r2.address, ok: r2.address === CUSTOMER_B });

const h3 = await exchange.setSubname({ parent: PARENT, label: "muxed", address: CUSTOMER_C }, EXCHANGE);
const r3 = await bob.resolve(`muxed.${PARENT}`);
step("a muxed subname resolves exactly", { name: `muxed.${PARENT}.sid`, hash: h3, resolved: r3.address, ok: r3.address === CUSTOMER_C });

try {
  await exchange.setPrimaryName(EXCHANGE, NAME);
  step("UNEXPECTED: the base account claimed a name that pays a virtual account", { ok: false });
} catch (e) {
  step("the base account can't claim a name paying its muxed address as primary name", {
    refusedWith: e.code ?? e.constructor.name,
    ok: e.code === "Unauthorized",
  });
}

const out = process.argv[2];
if (out) writeFileSync(out, JSON.stringify(receipt, null, 2) + "\n");
if (failed) process.exit(1);
