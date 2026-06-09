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
(: MinimalTransferToken Bundle)
(: ERC20ConformantToken Bundle)

(abstract-aspect BalanceAspect)
(aspect StaticBalanceAspect)
(aspect TransferableBalanceAspect)
(aspect SelfTransferAspect)
(aspect ApprovalAspect)
(aspect DelegatedTransferAspect)

(extends StaticBalanceAspect BalanceAspect)
(extends TransferableBalanceAspect BalanceAspect)
(extends SelfTransferAspect TransferableBalanceAspect)
(extends DelegatedTransferAspect TransferableBalanceAspect)

(owns BalanceAspect tokenDecimals UInt8)
(owns StaticBalanceAspect balanceOf (-> Address Amount))
(owns ApprovalAspect allowanceOf (-> Address Address Amount))

(provides TransferableBalanceAspect Balance-Transfer)
(provides ApprovalAspect Balance-Allowance)
(requires SelfTransferAspect Balance-Transfer)
(requires DelegatedTransferAspect Balance-Transfer)
(requires DelegatedTransferAspect Balance-Allowance)

(action TransferableBalanceAspect transferBalance)
(action SelfTransferAspect transferFromSelf)
(action ApprovalAspect approveAllowance)
(action ApprovalAspect spendAllowance)
(action DelegatedTransferAspect transferFromApproved)

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

(derived DelegatedTransferAspect transferFromApproved
  (= (transferFromApproved $owner $spender $to $amount)
     (begin
       (spendAllowance $owner $spender $amount)
       (transferBalance $owner $to $amount)
       True)))

(local-invariant TransferableBalanceAspect TBA-3
  True)

(local-invariant ApprovalAspect AA-1
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
