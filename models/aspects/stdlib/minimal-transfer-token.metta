; Minimal taxonomy-aligned executable transfer token fixture.
; It pins declaration parsing, comments, canonicalization, source-root behavior,
; and the first runtime-compatible balance transfer and allowance actions.

(: Aspect Type)
(: Bundle Type)
(: Facet Type)
(: State Type)
(: Action Type)
(: Invariant Type)
(: Projection Type)
(: Address Type)
(: Amount Type)
(: Bool Type)
(: UInt8 Type)

(: BalanceAspect Aspect)
(: StaticBalanceAspect Aspect)
(: TransferableBalanceAspect Aspect)
(: SelfTransferAspect Aspect)
(: ApprovalAspect Aspect)
(: DelegatedTransferAspect Aspect)
(: FeeTransferAspect Aspect)
(: PausableTransferAspect Aspect)
(: RestrictedTransferAspect Aspect)
(: LockedTransferAspect Aspect)
(: MintableBalanceAspect Aspect)
(: BurnableBalanceAspect Aspect)
(: CappedMintableAspect Aspect)
(: VotableBalanceAspect Aspect)
(: SnapshotBalanceAspect Aspect)
(: VaultShareBalanceAspect Aspect)
(: WrappedBalanceAspect Aspect)
(: StakeBalanceAspect Aspect)
(: RewardedStakeBalanceAspect Aspect)
(: BridgeMintBurnAspect Aspect)
(: MinimalTransferToken Bundle)
(: ERC20ConformantToken Bundle)
(: FeeToken Bundle)
(: PausableToken Bundle)
(: RestrictedToken Bundle)
(: LockedToken Bundle)
(: MintBurnToken Bundle)
(: CappedMintToken Bundle)
(: VotableToken Bundle)
(: SnapshotToken Bundle)
(: VaultShareToken Bundle)
(: WrappedToken Bundle)
(: RewardedStakeToken Bundle)
(: BridgeMintBurnToken Bundle)

(abstract-aspect BalanceAspect)
(aspect StaticBalanceAspect)
(aspect TransferableBalanceAspect)
(aspect SelfTransferAspect)
(aspect ApprovalAspect)
(aspect DelegatedTransferAspect)
(aspect FeeTransferAspect)
(aspect PausableTransferAspect)
(aspect RestrictedTransferAspect)
(aspect LockedTransferAspect)
(aspect MintableBalanceAspect)
(aspect BurnableBalanceAspect)
(aspect CappedMintableAspect)
(aspect VotableBalanceAspect)
(aspect SnapshotBalanceAspect)
(aspect VaultShareBalanceAspect)
(aspect WrappedBalanceAspect)
(aspect StakeBalanceAspect)
(aspect RewardedStakeBalanceAspect)
(aspect BridgeMintBurnAspect)

(extends StaticBalanceAspect BalanceAspect)
(extends TransferableBalanceAspect BalanceAspect)
(extends SelfTransferAspect TransferableBalanceAspect)
(extends DelegatedTransferAspect TransferableBalanceAspect)
(extends FeeTransferAspect TransferableBalanceAspect)
(extends PausableTransferAspect TransferableBalanceAspect)
(extends RestrictedTransferAspect TransferableBalanceAspect)
(extends LockedTransferAspect TransferableBalanceAspect)
(extends MintableBalanceAspect BalanceAspect)
(extends BurnableBalanceAspect BalanceAspect)
(extends CappedMintableAspect MintableBalanceAspect)
(extends VotableBalanceAspect BalanceAspect)
(extends SnapshotBalanceAspect BalanceAspect)
(extends VaultShareBalanceAspect BalanceAspect)
(extends WrappedBalanceAspect BalanceAspect)
(extends StakeBalanceAspect BalanceAspect)
(extends RewardedStakeBalanceAspect StakeBalanceAspect)
(extends BridgeMintBurnAspect BalanceAspect)

(owns BalanceAspect tokenDecimals UInt8)
(owns StaticBalanceAspect initialized Amount)
(owns StaticBalanceAspect totalSupply Amount)
(owns StaticBalanceAspect balanceOf (-> Address Amount))
(owns ApprovalAspect allowanceOf (-> Address Address Amount))
(owns FeeTransferAspect feeBps Amount)
(owns PausableTransferAspect paused Amount)
(owns RestrictedTransferAspect blocked (-> Address Amount))
(owns LockedTransferAspect lockedUntil (-> Address Amount))
(owns CappedMintableAspect mintCap Amount)
(owns SnapshotBalanceAspect snapshotBalanceOf (-> Amount Address Amount))
(owns SnapshotBalanceAspect snapshotSupply (-> Amount Amount))
(owns VaultShareBalanceAspect vaultAssetReserve Amount)
(owns VaultShareBalanceAspect vaultShareSupply Amount)
(owns VaultShareBalanceAspect vaultShareBalanceOf (-> Address Amount))
(owns WrappedBalanceAspect wrappedUnderlyingReserve Amount)
(owns StakeBalanceAspect stakeBalanceOf (-> Address Amount))
(owns StakeBalanceAspect totalStakedBalance Amount)
(owns RewardedStakeBalanceAspect rewardPerBlock Amount)
(owns RewardedStakeBalanceAspect pendingRewardOf (-> Address Amount))
(owns RewardedStakeBalanceAspect lastRewardHeightOf (-> Address Amount))
(owns BridgeMintBurnAspect bridgeMessageConsumed (-> Address Amount))

(provides TransferableBalanceAspect Balance-Transfer)
(provides ApprovalAspect Balance-Allowance)
(provides FeeTransferAspect Fee-Transfer)
(provides PausableTransferAspect Pause-Control)
(provides RestrictedTransferAspect Restriction-Control)
(provides LockedTransferAspect Transfer-Lock)
(provides MintableBalanceAspect Balance-Mint)
(provides BurnableBalanceAspect Balance-Burn)
(provides CappedMintableAspect Balance-CappedMint)
(provides VotableBalanceAspect Balance-Votes)
(provides SnapshotBalanceAspect Balance-Snapshot)
(provides VaultShareBalanceAspect Vault-Share)
(provides WrappedBalanceAspect Balance-Wrap)
(provides StakeBalanceAspect Balance-Stake)
(provides RewardedStakeBalanceAspect Balance-RewardedStake)
(provides BridgeMintBurnAspect Bridge-MintBurn)
(requires SelfTransferAspect Balance-Transfer)
(requires DelegatedTransferAspect Balance-Transfer)
(requires DelegatedTransferAspect Balance-Allowance)
(requires FeeTransferAspect Balance-Transfer)
(requires PausableTransferAspect Balance-Transfer)
(requires RestrictedTransferAspect Balance-Transfer)
(requires LockedTransferAspect Balance-Transfer)
(requires CappedMintableAspect Balance-Mint)
(requires RewardedStakeBalanceAspect Balance-Stake)
(requires BridgeMintBurnAspect Balance-Mint)
(requires BridgeMintBurnAspect Balance-Burn)

(action StaticBalanceAspect initializeSupply)
(action TransferableBalanceAspect transferBalance)
(action SelfTransferAspect transferFromSelf)
(action ApprovalAspect approveAllowance)
(action ApprovalAspect spendAllowance)
(action ApprovalAspect permitAllowance)
(action DelegatedTransferAspect transferFromApproved)
(action FeeTransferAspect configureFee)
(action FeeTransferAspect transferWithFee)
(action PausableTransferAspect setPaused)
(action PausableTransferAspect transferUnlessPaused)
(action RestrictedTransferAspect setBlocked)
(action RestrictedTransferAspect transferUnlessBlocked)
(action LockedTransferAspect setLockedUntil)
(action LockedTransferAspect transferAfterUnlock)
(action MintableBalanceAspect mintBalance)
(action BurnableBalanceAspect burnBalance)
(action CappedMintableAspect setMintCap)
(action CappedMintableAspect mintWithinCap)
(action VotableBalanceAspect readVotingPower)
(action SnapshotBalanceAspect captureSnapshot)
(action SnapshotBalanceAspect readSnapshotBalance)
(action SnapshotBalanceAspect readSnapshotSupply)
(action VaultShareBalanceAspect depositVault)
(action VaultShareBalanceAspect redeemVault)
(action VaultShareBalanceAspect readVaultShares)
(action WrappedBalanceAspect wrapBalance)
(action WrappedBalanceAspect unwrapBalance)
(action StakeBalanceAspect stakeBalance)
(action StakeBalanceAspect unstakeBalance)
(action StakeBalanceAspect readStakeBalance)
(action RewardedStakeBalanceAspect configureStakeReward)
(action RewardedStakeBalanceAspect accrueStakeReward)
(action RewardedStakeBalanceAspect stakeWithRewardAccrual)
(action RewardedStakeBalanceAspect unstakeWithRewardAccrual)
(action RewardedStakeBalanceAspect claimStakeReward)
(action BridgeMintBurnAspect bridgeMintBalance)
(action BridgeMintBurnAspect bridgeBurnBalance)

(derived StaticBalanceAspect initializeSupply
  (= (initializeSupply $owner $amount)
     (begin
       (require (= (state-get initialized) 0) ALREADY_INITIALIZED)
       (state-set! initialized 1)
       (state-set! totalSupply $amount)
       (state-set! balanceOf $owner $amount)
       (emit! (Transfer ZeroAddress $owner $amount))
       True)))

(derived TransferableBalanceAspect transferBalance
  (= (transferBalance $from $to $amount)
     (begin
       (require (>= (state-get balanceOf $from) $amount) INSUFFICIENT_BALANCE)
       (state-set! balanceOf $from
         (safe-sub (state-get balanceOf $from) $amount))
       (state-set! balanceOf $to
         (safe-add (state-get balanceOf $to) $amount))
       (emit! (Transfer $from $to $amount))
       True)))

(derived SelfTransferAspect transferFromSelf
  (= (transferFromSelf $caller $to $amount)
     (transferBalance $caller $to $amount)))

(derived ApprovalAspect approveAllowance
  (= (approveAllowance $owner $spender $amount)
     (begin
       (state-set! allowanceOf $owner $spender $amount)
       (emit! (Approval $owner $spender $amount))
       True)))

(derived ApprovalAspect spendAllowance
  (= (spendAllowance $owner $spender $amount)
     (begin
       (require (>= (state-get allowanceOf $owner $spender) $amount) ALLOWANCE_EXCEEDED)
       (state-set! allowanceOf $owner $spender
         (safe-sub (state-get allowanceOf $owner $spender) $amount))
       True)))

(derived ApprovalAspect permitAllowance
  (= (permitAllowance $owner $spender $asset $amount $certificate)
     (begin
       (permit-verify! $owner $spender $asset $amount $certificate)
       (state-set! allowanceOf $owner $spender $amount)
       (emit! (Approval $owner $spender $amount))
       True)))

(derived DelegatedTransferAspect transferFromApproved
  (= (transferFromApproved $owner $spender $to $amount)
     (begin
       (spendAllowance $owner $spender $amount)
       (transferBalance $owner $to $amount)
       True)))

(derived FeeTransferAspect configureFee
  (= (configureFee $feeBps)
     (begin
       (require (<= $feeBps 10000) FEE_TOO_HIGH)
       (state-set! feeBps $feeBps)
       True)))

(derived FeeTransferAspect transferWithFee
  (= (transferWithFee $from $to $amount)
     (begin
       (require (<= (state-get feeBps) 10000) FEE_TOO_HIGH)
       (require (not (= $from FeeTreasury)) FEE_TREASURY_SENDER_UNSUPPORTED)
       (require (not (= $to FeeTreasury)) FEE_TREASURY_RECIPIENT_UNSUPPORTED)
       (require (>= (state-get balanceOf $from) $amount) INSUFFICIENT_BALANCE)
       (state-set! balanceOf $from
         (safe-sub (state-get balanceOf $from) $amount))
       (state-set! balanceOf FeeTreasury
         (safe-add (state-get balanceOf FeeTreasury)
           (safe-div (safe-mul $amount (state-get feeBps)) 10000)))
       (state-set! balanceOf $to
         (safe-add (state-get balanceOf $to)
           (safe-sub $amount
             (safe-div (safe-mul $amount (state-get feeBps)) 10000))))
       (emit! (Transfer $from FeeTreasury
         (safe-div (safe-mul $amount (state-get feeBps)) 10000)))
       (emit! (Transfer $from $to
         (safe-sub $amount
           (safe-div (safe-mul $amount (state-get feeBps)) 10000))))
       True)))

(derived PausableTransferAspect setPaused
  (= (setPaused $paused)
     (begin
       (require (<= $paused 1) INVALID_PAUSE_FLAG)
       (state-set! paused $paused)
       True)))

(derived PausableTransferAspect transferUnlessPaused
  (= (transferUnlessPaused $from $to $amount)
     (begin
       (require (= (state-get paused) 0) TOKEN_PAUSED)
       (transferBalance $from $to $amount)
       True)))

(derived RestrictedTransferAspect setBlocked
  (= (setBlocked $account $blocked)
     (begin
       (require (<= $blocked 1) INVALID_BLOCK_FLAG)
       (state-set! blocked $account $blocked)
       True)))

(derived RestrictedTransferAspect transferUnlessBlocked
  (= (transferUnlessBlocked $from $to $amount)
     (begin
       (require (= (state-get blocked $from) 0) SENDER_BLOCKED)
       (require (= (state-get blocked $to) 0) RECIPIENT_BLOCKED)
       (transferBalance $from $to $amount)
       True)))

(derived LockedTransferAspect setLockedUntil
  (= (setLockedUntil $account $height)
     (begin
       (state-set! lockedUntil $account $height)
       True)))

(derived LockedTransferAspect transferAfterUnlock
  (= (transferAfterUnlock $from $to $amount)
     (begin
       (require (>= block-height (state-get lockedUntil $from)) SENDER_LOCKED)
       (transferBalance $from $to $amount)
       True)))

(derived MintableBalanceAspect mintBalance
  (= (mintBalance $to $amount)
     (begin
       (require (> $amount 0) ZERO_MINT)
       (state-set! totalSupply
         (safe-add (state-get totalSupply) $amount))
       (state-set! balanceOf $to
         (safe-add (state-get balanceOf $to) $amount))
       (emit! (Transfer ZeroAddress $to $amount))
       True)))

(derived BurnableBalanceAspect burnBalance
  (= (burnBalance $from $amount)
     (begin
       (require (> $amount 0) ZERO_BURN)
       (require (>= (state-get balanceOf $from) $amount) INSUFFICIENT_BALANCE)
       (require (>= (state-get totalSupply) $amount) INSUFFICIENT_SUPPLY)
       (state-set! balanceOf $from
         (safe-sub (state-get balanceOf $from) $amount))
       (state-set! totalSupply
         (safe-sub (state-get totalSupply) $amount))
       (emit! (Transfer $from ZeroAddress $amount))
       True)))

(derived CappedMintableAspect setMintCap
  (= (setMintCap $cap)
     (begin
       (require (>= $cap (state-get totalSupply)) CAP_BELOW_SUPPLY)
       (state-set! mintCap $cap)
       True)))

(derived CappedMintableAspect mintWithinCap
  (= (mintWithinCap $to $amount)
     (begin
       (require (<= (safe-add (state-get totalSupply) $amount) (state-get mintCap)) MINT_CAP_EXCEEDED)
       (mintBalance $to $amount)
       True)))

(derived VotableBalanceAspect readVotingPower
  (= (readVotingPower $account)
     (state-get balanceOf $account)))

(derived SnapshotBalanceAspect captureSnapshot
  (= (captureSnapshot $account $snapshotId)
     (begin
       (state-set! snapshotBalanceOf $snapshotId $account
         (state-get balanceOf $account))
       (state-set! snapshotSupply $snapshotId
         (state-get totalSupply))
       (emit! (Snapshot $snapshotId $account (state-get balanceOf $account)))
       True)))

(derived SnapshotBalanceAspect readSnapshotBalance
  (= (readSnapshotBalance $account $snapshotId)
     (state-get snapshotBalanceOf $snapshotId $account)))

(derived SnapshotBalanceAspect readSnapshotSupply
  (= (readSnapshotSupply $snapshotId)
     (state-get snapshotSupply $snapshotId)))

(derived VaultShareBalanceAspect depositVault
  (= (depositVault $account $assets)
     (begin
       (require (> $assets 0) ZERO_DEPOSIT)
       (if (= (state-get vaultShareSupply) 0)
         (begin
           (state-set! vaultAssetReserve
             (safe-add (state-get vaultAssetReserve) $assets))
           (state-set! vaultShareSupply
             (safe-add (state-get vaultShareSupply) $assets))
           (state-set! vaultShareBalanceOf $account
             (safe-add (state-get vaultShareBalanceOf $account) $assets))
           (emit! (VaultDeposit $account $assets $assets))
           $assets)
         (begin
           (require (> (state-get vaultAssetReserve) 0) EMPTY_RESERVE)
           (state-set! vaultAssetReserve
             (safe-add (state-get vaultAssetReserve) $assets))
           (state-set! vaultShareSupply
             (safe-add (state-get vaultShareSupply)
               (safe-div (safe-mul $assets (state-get vaultShareSupply))
                 (state-get vaultAssetReserve))))
           (state-set! vaultShareBalanceOf $account
             (safe-add (state-get vaultShareBalanceOf $account)
               (safe-div (safe-mul $assets (state-get vaultShareSupply))
                 (state-get vaultAssetReserve))))
           (emit! (VaultDeposit $account $assets
             (safe-div (safe-mul $assets (state-get vaultShareSupply))
               (state-get vaultAssetReserve))))
           (safe-div (safe-mul $assets (state-get vaultShareSupply))
             (state-get vaultAssetReserve)))))))

(derived VaultShareBalanceAspect redeemVault
  (= (redeemVault $account $shares)
     (begin
       (require (> $shares 0) ZERO_REDEEM)
       (require (> (state-get vaultShareSupply) 0) EMPTY_SHARE_SUPPLY)
       (require (>= (state-get vaultShareBalanceOf $account) $shares) INSUFFICIENT_SHARES)
       (require (>= (state-get vaultShareSupply) $shares) INSUFFICIENT_SHARE_SUPPLY)
       (state-set! vaultShareBalanceOf $account
         (safe-sub (state-get vaultShareBalanceOf $account) $shares))
       (state-set! vaultShareSupply
         (safe-sub (state-get vaultShareSupply) $shares))
       (state-set! vaultAssetReserve
         (safe-sub (state-get vaultAssetReserve)
           (safe-div (safe-mul $shares (state-get vaultAssetReserve))
             (state-get vaultShareSupply))))
       (emit! (VaultRedeem $account
         (safe-div (safe-mul $shares (state-get vaultAssetReserve))
           (state-get vaultShareSupply))
         $shares))
       (safe-div (safe-mul $shares (state-get vaultAssetReserve))
         (state-get vaultShareSupply)))))

(derived VaultShareBalanceAspect readVaultShares
  (= (readVaultShares $account)
     (state-get vaultShareBalanceOf $account)))

(derived WrappedBalanceAspect wrapBalance
  (= (wrapBalance $account $amount)
     (begin
       (require (> $amount 0) ZERO_WRAP)
       (state-set! wrappedUnderlyingReserve
         (safe-add (state-get wrappedUnderlyingReserve) $amount))
       (state-set! totalSupply
         (safe-add (state-get totalSupply) $amount))
       (state-set! balanceOf $account
         (safe-add (state-get balanceOf $account) $amount))
       (emit! (Transfer ZeroAddress $account $amount))
       (emit! (Wrap $account $amount))
       $amount)))

(derived WrappedBalanceAspect unwrapBalance
  (= (unwrapBalance $account $amount)
     (begin
       (require (> $amount 0) ZERO_UNWRAP)
       (require (>= (state-get balanceOf $account) $amount) INSUFFICIENT_BALANCE)
       (require (>= (state-get totalSupply) $amount) INSUFFICIENT_SUPPLY)
       (require (>= (state-get wrappedUnderlyingReserve) $amount) INSUFFICIENT_RESERVE)
       (state-set! balanceOf $account
         (safe-sub (state-get balanceOf $account) $amount))
       (state-set! totalSupply
         (safe-sub (state-get totalSupply) $amount))
       (state-set! wrappedUnderlyingReserve
         (safe-sub (state-get wrappedUnderlyingReserve) $amount))
       (emit! (Transfer $account ZeroAddress $amount))
       (emit! (Unwrap $account $amount))
       $amount)))

(derived StakeBalanceAspect stakeBalance
  (= (stakeBalance $account $amount)
     (begin
       (require (> $amount 0) ZERO_STAKE)
       (state-set! stakeBalanceOf $account
         (safe-add (state-get stakeBalanceOf $account) $amount))
       (state-set! totalStakedBalance
         (safe-add (state-get totalStakedBalance) $amount))
       (emit! (Stake $account $amount))
       True)))

(derived StakeBalanceAspect unstakeBalance
  (= (unstakeBalance $account $amount)
     (begin
       (require (> $amount 0) ZERO_UNSTAKE)
       (require (>= (state-get stakeBalanceOf $account) $amount) INSUFFICIENT_STAKE)
       (require (>= (state-get totalStakedBalance) $amount) INSUFFICIENT_TOTAL_STAKE)
       (state-set! stakeBalanceOf $account
         (safe-sub (state-get stakeBalanceOf $account) $amount))
       (state-set! totalStakedBalance
         (safe-sub (state-get totalStakedBalance) $amount))
       (emit! (Unstake $account $amount))
       True)))

(derived StakeBalanceAspect readStakeBalance
  (= (readStakeBalance $account)
     (state-get stakeBalanceOf $account)))

(derived RewardedStakeBalanceAspect configureStakeReward
  (= (configureStakeReward $reward)
     (begin
       (state-set! rewardPerBlock $reward)
       True)))

(derived RewardedStakeBalanceAspect accrueStakeReward
  (= (accrueStakeReward $account)
     (begin
       (require (>= block-height (state-get lastRewardHeightOf $account)) REWARD_TIME_REWIND)
       (state-set! pendingRewardOf $account
         (safe-add (state-get pendingRewardOf $account)
           (safe-mul
             (safe-mul
               (safe-sub block-height (state-get lastRewardHeightOf $account))
               (state-get stakeBalanceOf $account))
             (state-get rewardPerBlock))))
       (state-set! lastRewardHeightOf $account block-height)
       True)))

(derived RewardedStakeBalanceAspect stakeWithRewardAccrual
  (= (stakeWithRewardAccrual $account $amount)
     (begin
       (accrueStakeReward $account)
       (stakeBalance $account $amount)
       True)))

(derived RewardedStakeBalanceAspect unstakeWithRewardAccrual
  (= (unstakeWithRewardAccrual $account $amount)
     (begin
       (accrueStakeReward $account)
       (unstakeBalance $account $amount)
       True)))

(derived RewardedStakeBalanceAspect claimStakeReward
  (= (claimStakeReward $account)
     (begin
       (require (>= block-height (state-get lastRewardHeightOf $account)) REWARD_TIME_REWIND)
       (state-set! pendingRewardOf $account 0)
       (state-set! lastRewardHeightOf $account block-height)
       (emit! (RewardClaimed $account
         (safe-add (state-get pendingRewardOf $account)
           (safe-mul
             (safe-mul
               (safe-sub block-height (state-get lastRewardHeightOf $account))
               (state-get stakeBalanceOf $account))
             (state-get rewardPerBlock)))))
       (safe-add (state-get pendingRewardOf $account)
         (safe-mul
           (safe-mul
             (safe-sub block-height (state-get lastRewardHeightOf $account))
            (state-get stakeBalanceOf $account))
           (state-get rewardPerBlock))))))

(derived BridgeMintBurnAspect bridgeMintBalance
  (= (bridgeMintBalance $sourceChain $messageId $recipient $asset $amount $certificate)
     (begin
       (require (> $amount 0) ZERO_BRIDGE_MINT)
       (bridge-verify! $sourceChain $messageId $recipient $asset $amount $certificate)
       (require (= (state-get bridgeMessageConsumed $messageId) 0) BRIDGE_MESSAGE_REPLAY)
       (state-set! bridgeMessageConsumed $messageId 1)
       (mintBalance $recipient $amount)
       True)))

(derived BridgeMintBurnAspect bridgeBurnBalance
  (= (bridgeBurnBalance $account $amount)
     (begin
       (require (> $amount 0) ZERO_BRIDGE_BURN)
       (burnBalance $account $amount)
       True)))

(local-invariant TransferableBalanceAspect TBA-3
  True)

(local-invariant ApprovalAspect AA-1
  True)

(local-invariant FeeTransferAspect FEE-1
  True)

(local-invariant PausableTransferAspect PAUSE-1
  True)

(local-invariant RestrictedTransferAspect RESTRICT-1
  True)

(local-invariant LockedTransferAspect LOCK-1
  True)

(local-invariant MintableBalanceAspect MINT-1
  True)

(local-invariant BurnableBalanceAspect BURN-1
  True)

(local-invariant CappedMintableAspect CAP-1
  True)

(local-invariant VotableBalanceAspect VOTE-1
  True)

(local-invariant SnapshotBalanceAspect SNAP-1
  True)

(local-invariant VaultShareBalanceAspect VAULT-1
  True)

(local-invariant WrappedBalanceAspect WRAP-1
  True)

(local-invariant StakeBalanceAspect STAKE-1
  True)

(local-invariant RewardedStakeBalanceAspect REWARD-1
  True)

(local-invariant BridgeMintBurnAspect BRIDGE-1
  True)

(bundle MinimalTransferToken)
(bundle-includes MinimalTransferToken StaticBalanceAspect)
(bundle-includes MinimalTransferToken TransferableBalanceAspect)
(bundle-includes MinimalTransferToken SelfTransferAspect)
(bundle-includes MinimalTransferToken ApprovalAspect)
(bundle-includes MinimalTransferToken DelegatedTransferAspect)

(bundle ERC20ConformantToken)
(bundle-includes ERC20ConformantToken StaticBalanceAspect)
(bundle-includes ERC20ConformantToken TransferableBalanceAspect)
(bundle-includes ERC20ConformantToken SelfTransferAspect)
(bundle-includes ERC20ConformantToken ApprovalAspect)
(bundle-includes ERC20ConformantToken DelegatedTransferAspect)

(bundle FeeToken)
(bundle-includes FeeToken StaticBalanceAspect)
(bundle-includes FeeToken TransferableBalanceAspect)
(bundle-includes FeeToken FeeTransferAspect)

(bundle PausableToken)
(bundle-includes PausableToken StaticBalanceAspect)
(bundle-includes PausableToken TransferableBalanceAspect)
(bundle-includes PausableToken PausableTransferAspect)

(bundle RestrictedToken)
(bundle-includes RestrictedToken StaticBalanceAspect)
(bundle-includes RestrictedToken TransferableBalanceAspect)
(bundle-includes RestrictedToken RestrictedTransferAspect)

(bundle LockedToken)
(bundle-includes LockedToken StaticBalanceAspect)
(bundle-includes LockedToken TransferableBalanceAspect)
(bundle-includes LockedToken LockedTransferAspect)

(bundle MintBurnToken)
(bundle-includes MintBurnToken StaticBalanceAspect)
(bundle-includes MintBurnToken TransferableBalanceAspect)
(bundle-includes MintBurnToken MintableBalanceAspect)
(bundle-includes MintBurnToken BurnableBalanceAspect)

(bundle CappedMintToken)
(bundle-includes CappedMintToken StaticBalanceAspect)
(bundle-includes CappedMintToken TransferableBalanceAspect)
(bundle-includes CappedMintToken MintableBalanceAspect)
(bundle-includes CappedMintToken CappedMintableAspect)

(bundle VotableToken)
(bundle-includes VotableToken StaticBalanceAspect)
(bundle-includes VotableToken TransferableBalanceAspect)
(bundle-includes VotableToken VotableBalanceAspect)

(bundle SnapshotToken)
(bundle-includes SnapshotToken StaticBalanceAspect)
(bundle-includes SnapshotToken TransferableBalanceAspect)
(bundle-includes SnapshotToken SnapshotBalanceAspect)

(bundle VaultShareToken)
(bundle-includes VaultShareToken VaultShareBalanceAspect)

(bundle WrappedToken)
(bundle-includes WrappedToken StaticBalanceAspect)
(bundle-includes WrappedToken TransferableBalanceAspect)
(bundle-includes WrappedToken WrappedBalanceAspect)

(bundle RewardedStakeToken)
(bundle-includes RewardedStakeToken StakeBalanceAspect)
(bundle-includes RewardedStakeToken RewardedStakeBalanceAspect)

(bundle BridgeMintBurnToken)
(bundle-includes BridgeMintBurnToken StaticBalanceAspect)
(bundle-includes BridgeMintBurnToken TransferableBalanceAspect)
(bundle-includes BridgeMintBurnToken MintableBalanceAspect)
(bundle-includes BridgeMintBurnToken BurnableBalanceAspect)
(bundle-includes BridgeMintBurnToken BridgeMintBurnAspect)

(projection MinimalTransferToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferFromSelf tx.sender $to $amount)))

(projection ERC20ConformantToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferFromSelf tx.sender $to $amount)))

(projection MinimalTransferToken ERC20-approve
  (= (ERC20.approve $spender $amount)
     (approveAllowance tx.sender $spender $amount)))

(projection ERC20ConformantToken ERC20-approve
  (= (ERC20.approve $spender $amount)
     (approveAllowance tx.sender $spender $amount)))

(projection MinimalTransferToken ERC20-transferFrom
  (= (ERC20.transferFrom $owner $to $amount)
     (transferFromApproved $owner tx.sender $to $amount)))

(projection ERC20ConformantToken ERC20-transferFrom
  (= (ERC20.transferFrom $owner $to $amount)
     (transferFromApproved $owner tx.sender $to $amount)))

(projection ERC20ConformantToken ERC20-permit
  (= (ERC20.permit $owner $spender $asset $amount $certificate)
     (permitAllowance $owner $spender $asset $amount $certificate)))

(projection ERC20ConformantToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection FeeToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection FeeToken Fee-setConfig
  (= (Fee.setConfig $feeBps)
     (configureFee $feeBps)))

(projection FeeToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferWithFee tx.sender $to $amount)))

(projection PausableToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection PausableToken Pause-setPaused
  (= (Pause.setPaused $paused)
     (setPaused $paused)))

(projection PausableToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferUnlessPaused tx.sender $to $amount)))

(projection RestrictedToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection RestrictedToken Restriction-setBlocked
  (= (Restriction.setBlocked $account $blocked)
     (setBlocked $account $blocked)))

(projection RestrictedToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferUnlessBlocked tx.sender $to $amount)))

(projection LockedToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection LockedToken Lock-setUntil
  (= (Lock.setUntil $account $height)
     (setLockedUntil $account $height)))

(projection LockedToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferAfterUnlock tx.sender $to $amount)))

(projection MintBurnToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection MintBurnToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferBalance tx.sender $to $amount)))

(projection MintBurnToken Mint-mint
  (= (Mint.mint $to $amount)
     (mintBalance $to $amount)))

(projection MintBurnToken Burn-burn
  (= (Burn.burn $amount)
     (burnBalance tx.sender $amount)))

(projection CappedMintToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection CappedMintToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferBalance tx.sender $to $amount)))

(projection CappedMintToken Cap-setMintCap
  (= (Cap.setMintCap $cap)
     (setMintCap $cap)))

(projection CappedMintToken Mint-mint
  (= (Mint.mint $to $amount)
     (mintWithinCap $to $amount)))

(projection VotableToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection VotableToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferBalance tx.sender $to $amount)))

(projection VotableToken Votes-getVotes
  (= (Votes.getVotes $account)
     (readVotingPower $account)))

(projection SnapshotToken ERC20-initialize
  (= (ERC20.initialize $owner $amount)
     (initializeSupply $owner $amount)))

(projection SnapshotToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferBalance tx.sender $to $amount)))

(projection SnapshotToken Snapshot-capture
  (= (Snapshot.capture $account $snapshotId)
     (captureSnapshot $account $snapshotId)))

(projection SnapshotToken Snapshot-balanceOf
  (= (Snapshot.balanceOf $account $snapshotId)
     (readSnapshotBalance $account $snapshotId)))

(projection SnapshotToken Snapshot-totalSupply
  (= (Snapshot.totalSupply $snapshotId)
     (readSnapshotSupply $snapshotId)))

(projection VaultShareToken Vault-deposit
  (= (Vault.deposit $assets)
     (depositVault tx.sender $assets)))

(projection VaultShareToken Vault-redeem
  (= (Vault.redeem $shares)
     (redeemVault tx.sender $shares)))

(projection VaultShareToken Vault-sharesOf
  (= (Vault.sharesOf $account)
     (readVaultShares $account)))

(projection WrappedToken Wrapped-wrap
  (= (Wrapped.wrap $amount)
     (wrapBalance tx.sender $amount)))

(projection WrappedToken Wrapped-unwrap
  (= (Wrapped.unwrap $amount)
     (unwrapBalance tx.sender $amount)))

(projection WrappedToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferBalance tx.sender $to $amount)))

(projection RewardedStakeToken Staking-configureReward
  (= (Staking.configureReward $reward)
     (configureStakeReward $reward)))

(projection RewardedStakeToken Staking-stake
  (= (Staking.stake $amount)
     (stakeWithRewardAccrual tx.sender $amount)))

(projection RewardedStakeToken Staking-unstake
  (= (Staking.unstake $amount)
     (unstakeWithRewardAccrual tx.sender $amount)))

(projection RewardedStakeToken Staking-claimReward
  (= (Staking.claimReward)
     (claimStakeReward tx.sender)))

(projection RewardedStakeToken Staking-stakeOf
  (= (Staking.stakeOf $account)
     (readStakeBalance $account)))

(projection BridgeMintBurnToken Bridge-mint
  (= (Bridge.mint $sourceChain $messageId $recipient $asset $amount $certificate)
     (bridgeMintBalance $sourceChain $messageId $recipient $asset $amount $certificate)))

(projection BridgeMintBurnToken Bridge-burn
  (= (Bridge.burn $amount)
     (bridgeBurnBalance tx.sender $amount)))

(projection BridgeMintBurnToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferBalance tx.sender $to $amount)))

(method-abi MinimalTransferToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi ERC20ConformantToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi MinimalTransferToken ERC20-approve
  (args (spender Address) (amount Amount))
  Bool)

(method-abi ERC20ConformantToken ERC20-approve
  (args (spender Address) (amount Amount))
  Bool)

(method-abi MinimalTransferToken ERC20-transferFrom
  (args (owner Address) (to Address) (amount Amount))
  Bool)

(method-abi ERC20ConformantToken ERC20-transferFrom
  (args (owner Address) (to Address) (amount Amount))
  Bool)

(method-abi ERC20ConformantToken ERC20-permit
  (args (owner Address) (spender Address) (asset Asset) (amount Amount) (certificate Bytes))
  Bool)

(method-abi ERC20ConformantToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi FeeToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi FeeToken Fee-setConfig
  (args (feeBps Amount))
  Bool)

(method-abi FeeToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi PausableToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi PausableToken Pause-setPaused
  (args (paused Amount))
  Bool)

(method-abi PausableToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi RestrictedToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi RestrictedToken Restriction-setBlocked
  (args (account Address) (blocked Amount))
  Bool)

(method-abi RestrictedToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi LockedToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi LockedToken Lock-setUntil
  (args (account Address) (height Amount))
  Bool)

(method-abi LockedToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi MintBurnToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi MintBurnToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi MintBurnToken Mint-mint
  (args (to Address) (amount Amount))
  Bool)

(method-abi MintBurnToken Burn-burn
  (args (amount Amount))
  Bool)

(method-abi CappedMintToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi CappedMintToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi CappedMintToken Cap-setMintCap
  (args (cap Amount))
  Bool)

(method-abi CappedMintToken Mint-mint
  (args (to Address) (amount Amount))
  Bool)

(method-abi VotableToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi VotableToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi VotableToken Votes-getVotes
  (args (account Address))
  Amount)

(method-abi SnapshotToken ERC20-initialize
  (args (owner Address) (amount Amount))
  Bool)

(method-abi SnapshotToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi SnapshotToken Snapshot-capture
  (args (account Address) (snapshotId Amount))
  Bool)

(method-abi SnapshotToken Snapshot-balanceOf
  (args (account Address) (snapshotId Amount))
  Amount)

(method-abi SnapshotToken Snapshot-totalSupply
  (args (snapshotId Amount))
  Amount)

(method-abi VaultShareToken Vault-deposit
  (args (assets Amount))
  Amount)

(method-abi VaultShareToken Vault-redeem
  (args (shares Amount))
  Amount)

(method-abi VaultShareToken Vault-sharesOf
  (args (account Address))
  Amount)

(method-abi WrappedToken Wrapped-wrap
  (args (amount Amount))
  Amount)

(method-abi WrappedToken Wrapped-unwrap
  (args (amount Amount))
  Amount)

(method-abi WrappedToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-abi RewardedStakeToken Staking-configureReward
  (args (reward Amount))
  Bool)

(method-abi RewardedStakeToken Staking-stake
  (args (amount Amount))
  Bool)

(method-abi RewardedStakeToken Staking-unstake
  (args (amount Amount))
  Bool)

(method-abi RewardedStakeToken Staking-claimReward
  (args)
  Amount)

(method-abi RewardedStakeToken Staking-stakeOf
  (args (account Address))
  Amount)

(method-abi BridgeMintBurnToken Bridge-mint
  (args
    (sourceChain Address)
    (messageId Address)
    (recipient Address)
    (asset Asset)
    (amount Amount)
    (certificate Bytes))
  Bool)

(method-abi BridgeMintBurnToken Bridge-burn
  (args (amount Amount))
  Bool)

(method-abi BridgeMintBurnToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-policy MinimalTransferToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy ERC20ConformantToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy MinimalTransferToken ERC20-approve
  TxSender
  (effects WriteState EmitEvent)
  (invariants AA-1))

(method-policy ERC20ConformantToken ERC20-approve
  TxSender
  (effects WriteState EmitEvent)
  (invariants AA-1))

(method-policy MinimalTransferToken ERC20-transferFrom
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 AA-1))

(method-policy ERC20ConformantToken ERC20-transferFrom
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 AA-1))

(method-policy ERC20ConformantToken ERC20-permit
  PermitCertificate
  (effects WriteState EmitEvent UsePermitCertificate)
  (invariants AA-1))

(method-policy ERC20ConformantToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy FeeToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy FeeToken Fee-setConfig
  GovernanceAdminGrant
  (effects WriteState)
  (invariants FEE-1))

(method-policy FeeToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 FEE-1))

(method-policy PausableToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy PausableToken Pause-setPaused
  GovernanceAdminGrant
  (effects WriteState)
  (invariants PAUSE-1))

(method-policy PausableToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 PAUSE-1))

(method-policy RestrictedToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy RestrictedToken Restriction-setBlocked
  GovernanceAdminGrant
  (effects WriteState)
  (invariants RESTRICT-1))

(method-policy RestrictedToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 RESTRICT-1))

(method-policy LockedToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy LockedToken Lock-setUntil
  GovernanceAdminGrant
  (effects WriteState)
  (invariants LOCK-1))

(method-policy LockedToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 LOCK-1))

(method-policy MintBurnToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy MintBurnToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3))

(method-policy MintBurnToken Mint-mint
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants MINT-1))

(method-policy MintBurnToken Burn-burn
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants BURN-1))

(method-policy CappedMintToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 CAP-1))

(method-policy CappedMintToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 CAP-1))

(method-policy CappedMintToken Cap-setMintCap
  GovernanceAdminGrant
  (effects ReadState WriteState)
  (invariants CAP-1))

(method-policy CappedMintToken Mint-mint
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants MINT-1 CAP-1))

(method-policy VotableToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 VOTE-1))

(method-policy VotableToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 VOTE-1))

(method-policy VotableToken Votes-getVotes
  TxSender
  (effects ReadState)
  (invariants VOTE-1))

(method-policy SnapshotToken ERC20-initialize
  GovernanceAdminGrant
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 SNAP-1))

(method-policy SnapshotToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 SNAP-1))

(method-policy SnapshotToken Snapshot-capture
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants SNAP-1))

(method-policy SnapshotToken Snapshot-balanceOf
  TxSender
  (effects ReadState)
  (invariants SNAP-1))

(method-policy SnapshotToken Snapshot-totalSupply
  TxSender
  (effects ReadState)
  (invariants SNAP-1))

(method-policy VaultShareToken Vault-deposit
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants VAULT-1))

(method-policy VaultShareToken Vault-redeem
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants VAULT-1))

(method-policy VaultShareToken Vault-sharesOf
  TxSender
  (effects ReadState)
  (invariants VAULT-1))

(method-policy WrappedToken Wrapped-wrap
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants WRAP-1))

(method-policy WrappedToken Wrapped-unwrap
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants WRAP-1))

(method-policy WrappedToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 WRAP-1))

(method-policy RewardedStakeToken Staking-configureReward
  GovernanceAdminGrant
  (effects WriteState)
  (invariants REWARD-1))

(method-policy RewardedStakeToken Staking-stake
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants STAKE-1 REWARD-1))

(method-policy RewardedStakeToken Staking-unstake
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants STAKE-1 REWARD-1))

(method-policy RewardedStakeToken Staking-claimReward
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants REWARD-1))

(method-policy RewardedStakeToken Staking-stakeOf
  TxSender
  (effects ReadState)
  (invariants STAKE-1))

(method-policy BridgeMintBurnToken Bridge-mint
  BridgeCertificate
  (effects ReadState WriteState EmitEvent UseBridgeCertificate)
  (invariants MINT-1 BRIDGE-1))

(method-policy BridgeMintBurnToken Bridge-burn
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants BURN-1 BRIDGE-1))

(method-policy BridgeMintBurnToken ERC20-transfer
  TxSender
  (effects ReadState WriteState EmitEvent)
  (invariants TBA-3 BRIDGE-1))
