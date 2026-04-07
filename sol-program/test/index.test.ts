import { expect, test } from "bun:test";
import {
  Connection, Keypair, LAMPORTS_PER_SOL, PublicKey,
  SystemProgram, Transaction, TransactionInstruction,
  sendAndConfirmTransaction,
  type CreateAccountParams,
} from "@solana/web3.js";
import { SIZE } from "./type";

const PUB_KEY = new PublicKey("5oiPHB5bLu4QF5B1fdcShAc8uNcn1VRkezLNpHUSnPPm");
const connection = new Connection("http://127.0.0.1:8899", "confirmed");

// Helper to serialize the Operation enum in Borsh format
function serializeOperation(op: "Add" | "Sub", value: number): Buffer {
  const buf = Buffer.alloc(5); // 1 byte enum index + 4 bytes u32
  buf.writeUInt8(op === "Add" ? 0 : 1, 0); // enum variant index
  buf.writeUInt32LE(value, 1);              // u32 value (little-endian)
  return buf;
}

test("init + add + sub", async () => {
  // --- 1. Setup: Create & fund user account ---
  const userAccount = Keypair.generate();
  const airdropSignature = await connection.requestAirdrop(
    userAccount.publicKey,
    10 * LAMPORTS_PER_SOL
  );
  await connection.confirmTransaction(airdropSignature);

  // --- 2. Create the data account owned by your program ---
  const newAccount = Keypair.generate();
  const lamports = await connection.getMinimumBalanceForRentExemption(SIZE);

  const createAccountTx = new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: userAccount.publicKey,
      newAccountPubkey: newAccount.publicKey,
      lamports,
      space: SIZE,
      programId: PUB_KEY,
    })
  );

  await sendAndConfirmTransaction(connection, createAccountTx, [
    userAccount,
    newAccount,
  ]);
  console.log("✅ Data account created:", newAccount.publicKey.toBase58());

  // --- 3. Call Add(5) ---
  const addInstruction = new TransactionInstruction({
    keys: [
      {
        pubkey: newAccount.publicKey,
        isSigner: false,
        isWritable: true, // we're writing to this account
      },
    ],
    programId: PUB_KEY,
    data: serializeOperation("Add", 5),
  });

  const addTx = new Transaction().add(addInstruction);
  await sendAndConfirmTransaction(connection, addTx, [userAccount]);
  console.log("✅ Add(5) executed");

  // --- 4. Read the counter value ---
  let accountInfo = await connection.getAccountInfo(newAccount.publicKey);
  let count = accountInfo!.data.readUInt32LE(0); // Counter { count: u32 }
  console.log("Counter after Add(5):", count);
  expect(count).toBe(5);

  // --- 5. Call Sub(2) ---
  const subInstruction = new TransactionInstruction({
    keys: [
      {
        pubkey: newAccount.publicKey,
        isSigner: false,
        isWritable: true,
      },
    ],
    programId: PUB_KEY,
    data: serializeOperation("Sub", 2),
  });

  const subTx = new Transaction().add(subInstruction);
  await sendAndConfirmTransaction(connection, subTx, [userAccount]);
  console.log("✅ Sub(2) executed");

  // --- 6. Verify final value ---
  accountInfo = await connection.getAccountInfo(newAccount.publicKey);
  count = accountInfo!.data.readUInt32LE(0);
  console.log("Counter after Sub(2):", count);
  expect(count).toBe(3); // 5 - 2 = 3
});