import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import BN from "bn.js";
import { PublicKey, Keypair, SystemProgram, ComputeBudgetProgram } from "@solana/web3.js";
import * as fs from "fs";

describe("bankai-salana settlement", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.BankaiSolana as Program<any>;

  it("initializes and settles a single order", async () => {
    const proof = JSON.parse(fs.readFileSync("proof.json", "utf8"));

    const vkeyHash = Buffer.from(proof.vkey.replace(/^0x/, ""), "hex");
    console.log(vkeyHash);

    const [statePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("state")],
      program.programId
    );

    await program.methods
      .initialize([...vkeyHash])
      .accounts({
        state: statePda,
        payer: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    // pick first order in proofsBySourceChain
    const firstKey = Object.keys(proof.proofsBySourceChain)[0];
    const op = proof.proofsBySourceChain[firstKey][0];

    // transform order into program types
    const amountHex = BigInt(op.order.amount).toString(16).padStart(64, "0");
    const receiverBuf = Buffer.from(op.order.receiver.replace(/^0x/, ""), "hex");
    const amountBuf = Buffer.from(amountHex, "hex");
    const order = {
      sourceChainId: new BN(op.order.source_chain_id),
      destinationChainId: new BN(op.order.destination_chain_id),
      receiver: [...receiverBuf],
      amount: [...amountBuf],
      blockNumber: new BN(op.order.block_number),
    };

    const orderHash = Buffer.from(op.order_hash.replace(/^0x/, ""), "hex");
    const [orderPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("order"), orderHash],
      program.programId
    );

    await program.methods
      .submitOrder(order, [...orderHash])
      .accounts({
        state: statePda,
        orderStatus: orderPda,
        payer: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const pv = Buffer.from(proof.publicValues.replace(/^0x/, ""), "hex");
    const pf = Buffer.from(proof.proof.replace(/^0x/, ""), "hex");
    const merkleProof: number[][] = op.proof.map((h: string) =>
      [...Buffer.from(h.replace(/^0x/, ""), "hex")]
    );

    // 🧩 Add compute budget increase before settleOrders
    const modifyComputeUnits = ComputeBudgetProgram.setComputeUnitLimit({
      units: 1_400_000, // max allowed
    });

    const addPriorityFee = ComputeBudgetProgram.setComputeUnitPrice({
      microLamports: 1, // optional
    });

    // Build and send the transaction manually
    const tx = new anchor.web3.Transaction()
      .add(modifyComputeUnits)
      .add(addPriorityFee)
      .add(
        await program.methods
          .settleOrders(
            pv,
            pf,
            [{ order: order, orderHash: [...orderHash], proof: merkleProof }]
          )
          .accounts({ state: statePda })
          .remainingAccounts([
            { pubkey: orderPda, isWritable: true, isSigner: false },
          ])
          .instruction()
      );

    const sig = await provider.sendAndConfirm(tx, []);
    console.log("✅ Settlement tx:", sig);
  });
});
