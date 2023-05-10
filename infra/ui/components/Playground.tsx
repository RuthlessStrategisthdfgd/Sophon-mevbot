'use client'
import { useState } from 'react'
import { FAKE_TRANSACTION } from '@/components/TransactionBuilder'
import { FAKE_SIGNATURE } from '@/components/SignatureBuilder'
import Container from '@/components/Component'
import { createTransaction, signTransaction } from '@/lib/methods'
import RPCMethodPreview from '@/components/RPCMethodPreview'
import { useNodeContext } from '@/contexts/NodeProvider'
import StatsBlock from '@/components/StatsBlock'

const Playground = () => {
  const { fullState, memPool, nodeType, account, address, setAddress } =
    useNodeContext()

  const TXBuilder = () => {
    const [signature, setSignature] = useState<string>('')

    const TXInputs = () => {
      const [senderAddress, setSenderAddress] = useState<string>(
        FAKE_TRANSACTION.sender_address
      )
      const [senderPublicKey, setSenderPublicKey] = useState<string>(
        FAKE_TRANSACTION.sender_public_key
      )
      const [receiverAddress, setReceiverAddress] = useState<string>(
        FAKE_TRANSACTION.receiver_address
      )
      const [amount, setAmount] = useState<number>(FAKE_TRANSACTION.amount)
      const [nonce, setNonce] = useState<number>(FAKE_TRANSACTION.nonce)

      return (
        <>
          <div className={'flex flex-col gap-1 w-full'}>
            <label htmlFor="signature">Signature</label>
            <input
              readOnly
              onFocus={(e) => e.target.select()}
              type="text"
              name="signature"
              id="signature"
              placeholder="Signature"
              className="signature-input p-3 text-black border w-full rounded-xl"
              value={signature}
            />
          </div>
          <div className="w-full flex flex-wrap text-sm p-4 gap-1 justify-center">
            <div className={'flex flex-col gap-1'}>
              <label htmlFor="sender_address">Sender Address</label>
              <input
                onFocus={(e) => e.target.select()}
                type="text"
                name="sender_address"
                id="sender_address"
                placeholder="Sender Address"
                className="sender-address-input p-3 text-black border w-full text-[#000] rounded-xl"
                value={String(address)}
                onChange={(e) => setSenderAddress(e.target.value)}
              />
            </div>
            <div className={'flex flex-col gap-1'}>
              <label htmlFor="sender_public_key">Sender Public Key</label>
              <input
                onFocus={(e) => e.target.select()}
                type="text"
                name="sender_public_key"
                id="sender_public_key"
                placeholder="Sender Public Key"
                className="sender-public-key-input p-3 text-black border w-full rounded-xl"
                value={senderPublicKey}
                onChange={(e) => setSenderPublicKey(e.target.value)}
              />
            </div>
            <div className={'flex flex-col gap-1'}>
              <label htmlFor="receiver_address">Receiver Address</label>
              <input
                onFocus={(e) => e.target.select()}
                type="text"
                name="receiver_address"
                id="receiver_address"
                placeholder="Receiver Address"
                className="receiver-address-input p-3 text-black border w-full rounded-xl"
                value={receiverAddress}
                onChange={(e) => setReceiverAddress(e.target.value)}
              />
            </div>
            <div className={'flex flex-col gap-1'}>
              <label htmlFor="amount">Amount</label>
              <input
                onFocus={(e) => e.target.select()}
                type="text"
                name="amount"
                id="amount"
                placeholder="Amount"
                className="amount-input p-3 text-black border w-full rounded-xl"
                value={amount}
                onChange={(e) => setAmount(parseInt(e.target.value))}
              />
            </div>
            <div className={'flex flex-col gap-1'}>
              <label htmlFor="nonce">Nonce</label>
              <input
                onFocus={(e) => e.target.select()}
                type="text"
                name="nonce"
                id="nonce"
                placeholder="Nonce"
                className="nonce-input p-3 text-black border w-full rounded-xl"
                value={nonce}
                onChange={(e) => setNonce(parseInt(e.target.value))}
              />
            </div>
          </div>
        </>
      )
    }

    return (
      <div className={'flex flex-row gap-2'}>
        <div
          className={
            'text-sm w-[100%] flex flex-col items-center justify-center  bg-blue-200 rounded-xl p-4 gap-3'
          }
        >
          <TXInputs />
          <button
            onClick={() => signTransaction(FAKE_SIGNATURE)}
            className="bg-gradient hover:bg-purple-500 border-4 border border-purple-500 text-white font-bold py-2 px-4 rounded-xl"
          >
            Sign Transaction
          </button>
          <button
            onClick={() =>
              // @ts-ignore
              createTransaction({ ...FAKE_TRANSACTION, timestamp: Date.now() })
            }
            disabled={!signature}
            className="bg-gradient hover:bg-purple-500 border-4 border border-purple-500 text-white font-bold py-2 px-4 rounded-xl"
          >
            Send Transaction
          </button>
        </div>
      </div>
    )
  }

  const AccountSelectDropdown = () => {
    return (
      <div className={'text-gray-700 text-sm gap-2 flex items-center'}>
        Account Overview
        <select
          className="select select-sm w-full max-w-xs"
          onChange={(e) => setAddress(e.target.value)}
          value={String(address)}
        >
          <option disabled>Pick an account</option>
          {fullState &&
            Object.keys(fullState).map((key) => {
              return <option key={key}>{key}</option>
            })}
        </select>
      </div>
    )
  }

  const AccountOverview = () => {
    return (
      <div className={'flex flex-col gap-2'}>
        <AccountSelectDropdown />
        <div className={'flex flex-row gap-3'}>
          <StatsBlock suffix={'debits'}>{account?.debits ?? 'N/A'}</StatsBlock>
          <StatsBlock suffix={'credits'}>
            {account?.credits ?? 'N/A'}
          </StatsBlock>
          <StatsBlock suffix={'storage'}>
            {account?.storage ?? 'N/A'}
          </StatsBlock>
          <StatsBlock suffix={'code'}>{account?.code ?? 'N/A'}</StatsBlock>
          <StatsBlock suffix={'nonce'}>{account?.nonce ?? 'N/A'}</StatsBlock>
        </div>
      </div>
    )
  }

  const NodeOverview = () => {
    return (
      <div className={'flex flex-col gap-2'}>
        <div className={'text-gray-700 text-sm'}>Node Stats</div>
        <div className={'flex flex-row gap-3'}>
          <StatsBlock suffix={'txs in the mempool'}>
            {memPool.length ?? 'N/A'}
          </StatsBlock>
          <StatsBlock suffix={'node type'}>{nodeType ?? 'N/A'}</StatsBlock>
        </div>
      </div>
    )
  }

  const NodeInteraction = () => {
    return (
      <Container>
        <div className={'grid grid-cols-2 gap-4'}>
          <div className={'gap-4 flex-col flex'}>
            <NodeOverview />
            <AccountOverview />
          </div>
          <RPCMethodPreview address={address} />
        </div>
      </Container>
    )
  }

  return (
    <div className={'m-4 flex flex-col gap-2 '}>
      <NodeInteraction />
      <TXBuilder />
    </div>
  )
}

export default Playground
