import { describe,it,expect } from 'vitest'
import { parseOnchainAttempt } from '../peerOnchainPurchase'
const original={operation_id:'11111111-1111-4111-8111-111111111111',price_sats:546,phase:'template_prepared',network:'mainnet',external_exposure:false,address:null,fee_sats:142,max_fee_sats:1000,template_sha256:'a'.repeat(64),plan_sha256:null,txid:null,paid:false,change_allocation_ambiguous:false,can_switch_method:false,retired_unallocated:false}
describe('durable on-chain owner state',()=>{
  it('preserves original amount and fee for explicit confirmation',()=>expect(parseOnchainAttempt(original)).toEqual(original))
  it('rejects unknown state and incoherent exposure or fee metadata',()=>{
    for(const value of [{},{...original,can_switch_method:true},{...original,address:'bc1hidden'},{...original,external_exposure:true},{...original,fee_sats:1001},{...original,price_sats:545},{...original,template_sha256:'bad'},{...original,phase:'failed'}])expect(()=>parseOnchainAttempt(value)).toThrow()
  })
  it('does not equate allocation ambiguity with permission to switch',()=>{
    const result=parseOnchainAttempt({...original,phase:'quoted',fee_sats:null,max_fee_sats:null,template_sha256:null,change_allocation_ambiguous:true})
    expect(result.can_switch_method).toBe(false);expect(result.change_allocation_ambiguous).toBe(true)
  })
})

it('accepts only a coherent terminal unallocated acknowledgement for switching',()=>{
  const retired={...original,phase:'address_requested',network:null,fee_sats:null,max_fee_sats:null,template_sha256:null,retired_unallocated:true,can_switch_method:true}
  expect(parseOnchainAttempt(retired).can_switch_method).toBe(true)
  for(const value of [{...retired,address:'bc1issued'},{...retired,external_exposure:true},{...retired,paid:true},{...retired,change_allocation_ambiguous:true},{...retired,phase:'funded'},{...retired,txid:'b'.repeat(64)}])expect(()=>parseOnchainAttempt(value)).toThrow()
})

it('requires original plan confirmation and permits only unallocated plan retirement',()=>{
  const plan={...original,phase:'plan_prepared',template_sha256:null,plan_sha256:'c'.repeat(64)}
  expect(parseOnchainAttempt(plan).plan_sha256).toBe(plan.plan_sha256)
  expect(()=>parseOnchainAttempt({...plan,plan_sha256:null})).toThrow()
  expect(parseOnchainAttempt({...plan,can_switch_method:true,retired_unallocated:true}).can_switch_method).toBe(true)
  expect(()=>parseOnchainAttempt({...plan,phase:'plan_lease_dispatched',can_switch_method:true,retired_unallocated:true})).toThrow()
})
