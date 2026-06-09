; Minimal taxonomy-aligned source fixture for the first DeTTa aspect parser.
; This is not yet executable contract code. It pins declaration parsing,
; comments, canonicalization, and source-root behavior.

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

(derived SelfTransferAspect transferFromSelf
  (= (transferFromSelf $caller $to $amount)
     (transferBalance $caller $to $amount)))

(local-invariant TransferableBalanceAspect TBA-3
  (preserves transferBalance totalBalance))

(bundle MinimalTransferToken)
(bundle-includes MinimalTransferToken StaticBalanceAspect)
(bundle-includes MinimalTransferToken TransferableBalanceAspect)
(bundle-includes MinimalTransferToken SelfTransferAspect)

(projection MinimalTransferToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferFromSelf tx.sender $to $amount)))
