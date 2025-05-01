import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Stakingprototype } from "../target/types/stakingprototype";
import { expect } from "chai";
import { PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import { 
  TOKEN_PROGRAM_ID, 
  ASSOCIATED_TOKEN_PROGRAM_ID, 
  createMint, 
  createAccount,
  mintTo,
  getAccount
} from "@solana/spl-token";

describe("stakingprototype", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.Stakingprototype as Program<Stakingprototype>;
  const provider = anchor.getProvider();
  const adminWallet = (provider as anchor.AnchorProvider).wallet;
  
  let stakingPoolPda: PublicKey;
  let stakingPoolBump: number;
  let stakeMint: PublicKey;
  let rewardMint: PublicKey;
  let poolStakeAccount: PublicKey;
  let poolRewardAccount: PublicKey;
  
  // Create a second wallet for testing
  const userWallet = anchor.web3.Keypair.generate();
  let userStakePda: PublicKey;
  let userStakeAccount: PublicKey;
  let userRewardAccount: PublicKey;
  
  const rewardRate = new anchor.BN(10);
  const stakeAmount = new anchor.BN(1000);
  
  before(async () => {
    // Airdrop SOL to the user wallet
    const connection = provider.connection;
    const signature = await connection.requestAirdrop(
      userWallet.publicKey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await connection.confirmTransaction(signature);
    
    // Derive PDAs
    [stakingPoolPda, stakingPoolBump] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("staking_pool")],
      program.programId
    );
    
    [userStakePda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("user-stake"), userWallet.publicKey.toBuffer()],
      program.programId
    );
    
    // Create token mints
    stakeMint = await createMint(
      connection,
      (adminWallet as anchor.Wallet).payer,
      adminWallet.publicKey,
      null,
      9
    );
    
    rewardMint = await createMint(
      connection,
      (adminWallet as anchor.Wallet).payer,
      adminWallet.publicKey,
      null,
      9
    );
    
    // Create pool token accounts using normal accounts (not ATAs) for PDAs
    const poolStakeAccountKeypair = Keypair.generate();
    poolStakeAccount = poolStakeAccountKeypair.publicKey;
    
    await createAccount(
      connection,
      (adminWallet as anchor.Wallet).payer,
      stakeMint,
      stakingPoolPda,
      poolStakeAccountKeypair
    );
    
    const poolRewardAccountKeypair = Keypair.generate();
    poolRewardAccount = poolRewardAccountKeypair.publicKey;
    
    await createAccount(
      connection,
      (adminWallet as anchor.Wallet).payer,
      rewardMint,
      stakingPoolPda,
      poolRewardAccountKeypair
    );
    
    // Create user token accounts
    userStakeAccount = await createAccount(
      connection,
      (adminWallet as anchor.Wallet).payer,
      stakeMint,
      userWallet.publicKey
    );
    
    userRewardAccount = await createAccount(
      connection,
      (adminWallet as anchor.Wallet).payer,
      rewardMint,
      userWallet.publicKey
    );
    
    // Mint tokens to pool and user
    await mintTo(
      connection,
      (adminWallet as anchor.Wallet).payer,
      stakeMint,
      userStakeAccount,
      adminWallet.publicKey,
      10000
    );
    
    await mintTo(
      connection,
      (adminWallet as anchor.Wallet).payer,
      rewardMint,
      poolRewardAccount,
      adminWallet.publicKey,
      10000
    );
  });

  it("Initialize the staking pool", async () => {
    const tx = await program.methods
      .initialize(rewardRate)
      .accounts({
        stakingPool: stakingPoolPda,
        admin: adminWallet.publicKey,
        stakeMint,
        rewardMint,
        poolStakeAccount,
        poolRewardAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    
    console.log("Your transaction signature", tx);
    
    // Verify the staking pool was initialized correctly
    const stakingPool = await program.account.stakingPool.fetch(stakingPoolPda);
    expect(stakingPool.admin.toString()).to.equal(adminWallet.publicKey.toString());
    expect(stakingPool.rewardRate.toNumber()).to.equal(rewardRate.toNumber());
    expect(stakingPool.totalStaked.toNumber()).to.equal(0);
    expect(stakingPool.stakeMint.toString()).to.equal(stakeMint.toString());
    expect(stakingPool.rewardMint.toString()).to.equal(rewardMint.toString());
    expect(stakingPool.poolStakeAccount.toString()).to.equal(poolStakeAccount.toString());
    expect(stakingPool.poolRewardAccount.toString()).to.equal(poolRewardAccount.toString());
  });

  it("Lets a user stake tokens", async () => {
    // Get initial token balances
    const userBalanceBefore = (await getAccount(provider.connection, userStakeAccount)).amount;
    const poolBalanceBefore = (await getAccount(provider.connection, poolStakeAccount)).amount;
    
    const tx = await program.methods
      .stake(stakeAmount)
      .accounts({
        stakingPool: stakingPoolPda,
        userStake: userStakePda,
        user: userWallet.publicKey,
        userTokenAccount: userStakeAccount,
        poolStakeAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([userWallet])
      .rpc();
    
    console.log("Stake transaction signature", tx);
    
    // Verify the stake
    const userStake = await program.account.userStake.fetch(userStakePda);
    expect(userStake.owner.toString()).to.equal(userWallet.publicKey.toString());
    expect(userStake.stakeAmount.toNumber()).to.equal(stakeAmount.toNumber());
    
    // Verify the staking pool total was updated
    const stakingPool = await program.account.stakingPool.fetch(stakingPoolPda);
    expect(stakingPool.totalStaked.toNumber()).to.equal(stakeAmount.toNumber());
    
    // Verify token balances
    const userBalanceAfter = (await getAccount(provider.connection, userStakeAccount)).amount;
    const poolBalanceAfter = (await getAccount(provider.connection, poolStakeAccount)).amount;
    
    expect(Number(userBalanceAfter)).to.equal(Number(userBalanceBefore) - stakeAmount.toNumber());
    expect(Number(poolBalanceAfter)).to.equal(Number(poolBalanceBefore) + stakeAmount.toNumber());
  });

  it("Lets a user unstake tokens", async () => {
    const unstakeAmount = new anchor.BN(500);
    
    // Get initial token balances
    const userBalanceBefore = (await getAccount(provider.connection, userStakeAccount)).amount;
    const poolBalanceBefore = (await getAccount(provider.connection, poolStakeAccount)).amount;
    
    const tx = await program.methods
      .unstake(unstakeAmount)
      .accounts({
        stakingPool: stakingPoolPda,
        userStake: userStakePda,
        user: userWallet.publicKey,
        userTokenAccount: userStakeAccount,
        poolStakeAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([userWallet])
      .rpc();
    
    console.log("Unstake transaction signature", tx);
    
    // Verify the stake was reduced
    const userStake = await program.account.userStake.fetch(userStakePda);
    expect(userStake.stakeAmount.toNumber()).to.equal(stakeAmount.sub(unstakeAmount).toNumber());
    
    // Verify the staking pool total was updated
    const stakingPool = await program.account.stakingPool.fetch(stakingPoolPda);
    expect(stakingPool.totalStaked.toNumber()).to.equal(stakeAmount.sub(unstakeAmount).toNumber());
    
    // Verify token balances
    const userBalanceAfter = (await getAccount(provider.connection, userStakeAccount)).amount;
    const poolBalanceAfter = (await getAccount(provider.connection, poolStakeAccount)).amount;
    
    expect(Number(userBalanceAfter)).to.equal(Number(userBalanceBefore) + unstakeAmount.toNumber());
    expect(Number(poolBalanceAfter)).to.equal(Number(poolBalanceBefore) - unstakeAmount.toNumber());
  });

  it("Lets a user claim rewards", async () => {
    // We need to stake more tokens to accrue rewards faster for testing
    // Get initial token balances
    const userBalanceBefore = (await getAccount(provider.connection, userStakeAccount)).amount;
    const poolBalanceBefore = (await getAccount(provider.connection, poolStakeAccount)).amount;
    
    // Stake more tokens to generate more rewards
    const additionalStakeAmount = new anchor.BN(9000);
    
    await program.methods
      .stake(additionalStakeAmount)
      .accounts({
        stakingPool: stakingPoolPda,
        userStake: userStakePda,
        user: userWallet.publicKey,
        userTokenAccount: userStakeAccount,
        poolStakeAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([userWallet])
      .rpc();
    
    // Wait a bit to accrue some rewards
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Get initial reward token balances
    const userRewardBefore = (await getAccount(provider.connection, userRewardAccount)).amount;
    const poolRewardBefore = (await getAccount(provider.connection, poolRewardAccount)).amount;
    
    const tx = await program.methods
      .claimRewards()
      .accounts({
        stakingPool: stakingPoolPda,
        userStake: userStakePda,
        user: userWallet.publicKey,
        userRewardAccount,
        poolRewardAccount,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([userWallet])
      .rpc();
    
    console.log("Claim rewards transaction signature", tx);
    
    // Verify rewards were reset
    const userStake = await program.account.userStake.fetch(userStakePda);
    expect(userStake.rewardDebt.toNumber()).to.equal(0);
    
    // Verify token balances - the user should have more reward tokens, pool should have less
    const userRewardAfter = (await getAccount(provider.connection, userRewardAccount)).amount;
    const poolRewardAfter = (await getAccount(provider.connection, poolRewardAccount)).amount;
    
    expect(Number(userRewardAfter)).to.be.greaterThan(Number(userRewardBefore));
    expect(Number(poolRewardAfter)).to.be.lessThan(Number(poolRewardBefore));
  });

  it("Lets an admin update the reward rate", async () => {
    const newRewardRate = new anchor.BN(20);
    
    const tx = await program.methods
      .updateRewardRate(newRewardRate)
      .accounts({
        stakingPool: stakingPoolPda,
        admin: adminWallet.publicKey,
      })
      .rpc();
    
    console.log("Update reward rate transaction signature", tx);
    
    // Verify the reward rate was updated
    const stakingPool = await program.account.stakingPool.fetch(stakingPoolPda);
    expect(stakingPool.rewardRate.toNumber()).to.equal(newRewardRate.toNumber());
  });
});
