import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { TransferHook } from "../target/types/transfer_hook";
import {
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
  Keypair,
} from "@solana/web3.js";
import {
  ExtensionType,
  TOKEN_2022_PROGRAM_ID,
  getMintLen,
  createInitializeMintInstruction,
  createInitializeTransferHookInstruction,
  addExtraAccountsToInstruction,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  createMintToInstruction,
  createTransferCheckedInstruction,
  getAssociatedTokenAddressSync,
  createApproveInstruction,
  createSyncNativeInstruction,
  NATIVE_MINT,
  TOKEN_PROGRAM_ID,
  getAccount,
  getOrCreateAssociatedTokenAccount,
  createTransferCheckedWithTransferHookInstruction,
} from "@solana/spl-token";
import assert from "assert";
 
describe("transfer-hook", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
 
  const program = anchor.workspace.TransferHook as Program<TransferHook>;
  const wallet = provider.wallet as anchor.Wallet;
  const connection = provider.connection;
 
  // Generate keypair to use as address for the transfer-hook enabled mint
  const mint = new Keypair();
  const decimals = 9;
 
  // Sender token account address
  const sourceTokenAccount = getAssociatedTokenAddressSync(
    mint.publicKey,
    wallet.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID
  );
 
  // Recipient token account address
  const recipient = Keypair.generate();
  const destinationTokenAccount = getAssociatedTokenAddressSync(
    mint.publicKey,
    recipient.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID
  );
 
  // ExtraAccountMetaList address
  // Store extra accounts required by the custom transfer hook instruction
  const [extraAccountMetaListPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("extra-account-metas"), mint.publicKey.toBuffer()],
    program.programId
  );
 
  // PDA delegate to transfer wSOL tokens from sender
  const [delegatePDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("delegate")],
    program.programId
  );
 
  // Sender wSOL token account address
  const senderWSolTokenAccount = getAssociatedTokenAddressSync(
    NATIVE_MINT, // mint
    wallet.publicKey // owner
  );
 
  // Delegate PDA wSOL token account address, to receive wSOL tokens from sender
  const delegateWSolTokenAccount = getAssociatedTokenAddressSync(
    NATIVE_MINT, // mint
    delegatePDA, // owner
    true // allowOwnerOffCurve
  );
 
  // Create the two WSol token accounts as part of setup
  before(async () => {
    // WSol Token Account for sender
    await getOrCreateAssociatedTokenAccount(
      connection,
      wallet.payer,
      NATIVE_MINT,
      wallet.publicKey
    );
 
    // WSol Token Account for delegate PDA
    await getOrCreateAssociatedTokenAccount(
      connection,
      wallet.payer,
      NATIVE_MINT,
      delegatePDA,
      true
    );
  });
 
  it("Create Mint Account with Transfer Hook Extension", async () => {
    const extensions = [ExtensionType.TransferHook];
    const mintLen = getMintLen(extensions);
    const lamports =
      await provider.connection.getMinimumBalanceForRentExemption(mintLen);
   
    const transaction = new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: wallet.publicKey,
        newAccountPubkey: mint.publicKey,
        space: mintLen,
        lamports: lamports,
        programId: TOKEN_2022_PROGRAM_ID,
      }),
      createInitializeTransferHookInstruction(
        mint.publicKey,
        wallet.publicKey,
        program.programId, // Transfer Hook Program ID
        TOKEN_2022_PROGRAM_ID,
      ),
      createInitializeMintInstruction(
        mint.publicKey,
        decimals,
        wallet.publicKey,
        null,
        TOKEN_2022_PROGRAM_ID,
      ),
    );
   
    const txSig = await sendAndConfirmTransaction(
      provider.connection,
      transaction,
      [wallet.payer, mint],
    );
    console.log(`Transaction Signature: ${txSig}`);
  });
  // Create the two token accounts for the transfer-hook enabled mint
  // Fund the sender token account with 100 tokens
  it("Create Token Accounts and Mint Tokens", async () => {
    // 100 tokens
    const amount = 100 * 10 ** decimals;
  
    const transaction = new Transaction().add(
      createAssociatedTokenAccountInstruction(
        wallet.publicKey,
        sourceTokenAccount,
        wallet.publicKey,
        mint.publicKey,
        TOKEN_2022_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID,
      ),
      createAssociatedTokenAccountInstruction(
        wallet.publicKey,
        destinationTokenAccount,
        recipient.publicKey,
        mint.publicKey,
        TOKEN_2022_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID,
      ),
      createMintToInstruction(
        mint.publicKey,
        sourceTokenAccount,
        wallet.publicKey,
        amount,
        [],
        TOKEN_2022_PROGRAM_ID,
      ),
    );
  
    const txSig = await sendAndConfirmTransaction(
      connection,
      transaction,
      [wallet.payer],
      { skipPreflight: true },
    );
  
    console.log(`Transaction Signature: ${txSig}`);
  });
  // Account to store extra accounts required by the transfer hook instruction
  it("Create ExtraAccountMetaList Account", async () => {
    const initializeExtraAccountMetaListInstruction = await program.methods
      .initializeExtraAccountMetaList()
      .accounts({
        payer: wallet.publicKey,
        extraAccountMetaList: extraAccountMetaListPDA,
        mint: mint.publicKey,
        wsolMint: NATIVE_MINT,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .instruction();
  
    const transaction = new Transaction().add(
      initializeExtraAccountMetaListInstruction,
    );
  
    const txSig = await sendAndConfirmTransaction(
      provider.connection,
      transaction,
      [wallet.payer],
      { skipPreflight: true },
    );
    console.log("Transaction Signature:", txSig);
  });
  it("Transfer Hook with Extra Account Meta", async () => {
    // 1 tokens
    const amount = 0.1 * 10 ** decimals;
  
    // Instruction for sender to fund their WSol token account
    const solTransferInstruction = SystemProgram.transfer({
      fromPubkey: wallet.publicKey,
      toPubkey: senderWSolTokenAccount,
      lamports: amount,
    });
  
    // Approve delegate PDA to transfer WSol tokens from sender WSol token account
    const approveInstruction = createApproveInstruction(
      senderWSolTokenAccount,
      delegatePDA,
      wallet.publicKey,
      amount,
      [],
      TOKEN_PROGRAM_ID,
    );
  
    // Sync sender WSol token account
    const syncWrappedSolInstruction = createSyncNativeInstruction(
      senderWSolTokenAccount,
    );
  
    // This helper function will automatically derive all the additional accounts that were defined in the ExtraAccountMetas account
    let transferInstruction =
      await createTransferCheckedWithTransferHookInstruction(
        connection,
        sourceTokenAccount,
        mint.publicKey,
        destinationTokenAccount,
        wallet.publicKey,
        BigInt(100000000),
        decimals,
        [],
        "confirmed",
        TOKEN_2022_PROGRAM_ID,
      );
  
    const transaction = new Transaction().add(
      solTransferInstruction,
      syncWrappedSolInstruction,
      approveInstruction,
      transferInstruction,
    );
    try {
      const txSig = await sendAndConfirmTransaction(
        connection,
        transaction,
        [wallet.payer],
        { skipPreflight: true },
      );
      console.log("Transfer Signature:", txSig);
    } catch (error) {
      console.log(error)
    }
  });
  it("claim wsol", async() => {
    const claimAmount = 0.1 * 1 * 10 ** 9;
    try {
      const tx = await program.rpc.claim(
        new anchor.BN(claimAmount), {
          accounts: {
            user: wallet.payer.publicKey,
            delegate: delegatePDA,
            wsolMint: NATIVE_MINT,
            delegateWsolTokenAccount: delegateWSolTokenAccount,
            receiveWsolTokenAccount: senderWSolTokenAccount,
            tokenProgram: TOKEN_PROGRAM_ID,
            systemProgram: SystemProgram.programId
          },
          signers: [wallet.payer]
        }
      );
      console.log("tx->", tx);
    } catch (error) {
      console.log(error);
    }
  })
});