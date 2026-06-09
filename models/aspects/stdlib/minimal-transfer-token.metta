; Minimal taxonomy-aligned executable transfer token fixture.
; It pins declaration parsing, comments, canonicalization, source-root behavior,
; and the first runtime-compatible balance transfer action.

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
(: MinimalTransferToken Bundle)

(abstract-aspect BalanceAspect)
(aspect StaticBalanceAspect)
(aspect TransferableBalanceAspect)
(aspect SelfTransferAspect)

(extends StaticBalanceAspect BalanceAspect)
(extends TransferableBalanceAspect BalanceAspect)
(extends SelfTransferAspect TransferableBalanceAspect)

(owns BalanceAspect tokenDecimals UInt8)
(owns StaticBalanceAspect balanceOf (-> Address Amount))

(provides TransferableBalanceAspect Balance-Transfer)
(requires SelfTransferAspect Balance-Transfer)

(action TransferableBalanceAspect transferBalance)
(action SelfTransferAspect transferFromSelf)

(derived TransferableBalanceAspect transferBalance
  (= (transferBalance $from $to $amount)
     (begin
       (require (>= (state-get balanceOf $from) $amount) INSUFFICIENT_BALANCE)
       (state-set! balanceOf $from
         (safe-sub (state-get balanceOf $from) $amount))
       (state-set! balanceOf $to
         (safe-add (state-get balanceOf $to) $amount))
       True)))

(derived SelfTransferAspect transferFromSelf
  (= (transferFromSelf $caller $to $amount)
     (transferBalance $caller $to $amount)))

(local-invariant TransferableBalanceAspect TBA-3
  True)

(bundle MinimalTransferToken)
(bundle-includes MinimalTransferToken StaticBalanceAspect)
(bundle-includes MinimalTransferToken TransferableBalanceAspect)
(bundle-includes MinimalTransferToken SelfTransferAspect)

(projection MinimalTransferToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferFromSelf tx.sender $to $amount)))

(method-abi MinimalTransferToken ERC20-transfer
  (args (to Address) (amount Amount))
  Bool)

(method-policy MinimalTransferToken ERC20-transfer
  TxSender
  (effects ReadState WriteState)
  (invariants TBA-3))
