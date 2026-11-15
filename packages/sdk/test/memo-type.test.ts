import { beforeEach, describe, expect, it, vi } from "vitest";
import { Address, hash, Keypair, StrKey, xdr } from "@stellar/stellar-sdk";
import { SideraClient } from "../src/client.js";
import { parseMemoHint } from "../src/memo.js";
import { SideraError } from "../src/errors.js";

const OWNER = Keypair.random().publicKey();
const CONTRACT_ID = StrKey.encodeContract(hash(Buffer.from("sidera-memo-type-test")));

vi.mock("@stellar/stellar-sdk", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@stellar/stellar-sdk")>();
  return {
    ...actual,
    rpc: {
      ...actual.rpc,
      Server: vi.fn().mockImplementation(() => ({
        simulateTransaction: vi.fn(),
        getAccount: vi.fn(),
        sendTransaction: vi.fn(),
        getTransaction: vi.fn(),
        prepareTransaction: vi.fn(async (tx: unknown) => tx),
      })),
    },
  };
});

/** A `Resolution` as the upgraded contract returns it; `memoType` omitted = an old contract. */
function record(memo: string | null, memoType?: "Inferred" | "Id" | "Text") {
  const entry = (k: string, v: xdr.ScVal) => new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol(k), val: v });
  const entries = [
    entry("address", new Address(OWNER).toScVal()),
    entry("memo", memo == null ? xdr.ScVal.scvVoid() : xdr.ScVal.scvString(memo)),
  ];
  if (memoType) entries.push(entry("memo_type", xdr.ScVal.scvVec([xdr.ScVal.scvSymbol(memoType)])));
  return xdr.ScVal.scvMap(entries);
}

function client(signer = false) {
  const c = new SideraClient({
    contractId: CONTRACT_ID,
    network: "testnet",
    ...(signer ? { signer: { signTx: async (x: string) => x } } : {}),
  });
  return { c, server: (c as any).server };
}

describe("parseMemoHint with a stated type", () => {
  it("sends a numeric memo as text when the owner stated text", () => {
    expect(parseMemoHint("1001", "text")).toEqual({ type: "text", value: "1001", stated: true });
  });

  it("marks a stated id", () => {
    expect(parseMemoHint("1001", "id")).toEqual({ type: "id", value: "1001", stated: true });
  });

  it("refuses a stated id that is not a u64 instead of guessing", () => {
    expect(() => parseMemoHint("18446744073709551616", "id")).toThrow(SideraError);
    expect(() => parseMemoHint("abc", "id")).toThrow(SideraError);
  });

  it("still refuses a stated text memo longer than 28 bytes", () => {
    expect(() => parseMemoHint("x".repeat(29), "text")).toThrow(SideraError);
  });

  it("infers as before when nothing is stated", () => {
    expect(parseMemoHint("1001")).toEqual({ type: "id", value: "1001" });
    expect(parseMemoHint("ref-1")).toEqual({ type: "text", value: "ref-1" });
  });
});

describe("resolve with memo types", () => {
  beforeEach(() => vi.clearAllMocks());

  it("honours a stated MEMO_TEXT for a numeric memo", async () => {
    const { c, server } = client();
    server.simulateTransaction.mockResolvedValue({ result: { retval: record("1001", "Text") } });
    expect((await c.resolve("alice")).memo).toEqual({ type: "text", value: "1001", stated: true });
  });

  it("infers when the contract says Inferred", async () => {
    const { c, server } = client();
    server.simulateTransaction.mockResolvedValue({ result: { retval: record("1001", "Inferred") } });
    expect((await c.resolve("alice")).memo).toEqual({ type: "id", value: "1001" });
  });

  it("infers against a contract that predates memo types", async () => {
    const { c, server } = client();
    server.simulateTransaction.mockResolvedValue({ result: { retval: record("1001") } });
    expect((await c.resolve("alice")).memo).toEqual({ type: "id", value: "1001" });
  });

  it("applies stated types to subnames too", async () => {
    const { c, server } = client();
    const map = xdr.ScVal.scvMap([
      new xdr.ScMapEntry({ key: xdr.ScVal.scvString("alice"), val: record("1001", "Text") }),
    ]);
    server.simulateTransaction.mockResolvedValue({ result: { retval: map } });
    const [sub] = await c.subnames("exchange");
    expect(sub?.memo).toEqual({ type: "text", value: "1001", stated: true });
  });
});

describe("setMemoType", () => {
  beforeEach(() => vi.clearAllMocks());

  for (const [choice, variant] of [
    ["text", "Text"],
    ["id", "Id"],
    ["inferred", "Inferred"],
  ] as const) {
    it(`sends set_memo_type with the ${variant} variant`, async () => {
      const { c, server } = client(true);
      server.getAccount.mockResolvedValue({
        sequenceNumber: () => "1",
        accountId: () => OWNER,
        incrementSequenceNumber: () => {},
      });
      server.sendTransaction.mockResolvedValue({ hash: "tx1", status: "PENDING" });
      server.getTransaction.mockResolvedValue({ status: "SUCCESS", returnValue: xdr.ScVal.scvVoid() });

      expect(await c.setMemoType("alice.exchange.sid", choice, OWNER)).toBe("tx1");

      const hf = server.sendTransaction.mock.calls[0][0].operations[0].func.value();
      expect(hf.functionName().toString()).toBe("set_memo_type");
      const args = hf.args();
      expect(args[0].str().toString()).toBe("alice.exchange");
      expect(Address.fromScVal(args[1]).toString()).toBe(OWNER);
      expect(args[2].vec()![0].sym().toString()).toBe(variant);
    });
  }

  it("rejects an unknown type before any RPC", async () => {
    const { c, server } = client(true);
    await expect(c.setMemoType("alice", "hash" as any, OWNER)).rejects.toThrow(SideraError);
    expect(server.sendTransaction).not.toHaveBeenCalled();
  });
});
