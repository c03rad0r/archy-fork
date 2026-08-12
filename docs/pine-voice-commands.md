# Pine voice commands — the "what can I say" book

Everything here is spoken to the speaker after the wake word: **"Hey Jarvis, …"**

How it decides who answers you:
- **Exact-ish phrases** (sections 1–4) are matched **locally on your node** — instant,
  free, works with no internet and no API key.
- **Anything else** goes to **Claude ("Archy")**, which either calls the same node
  tools behind the scenes (so loose phrasings still get real numbers) or just
  answers the question. Needs the Anthropic API key that Pine seeds.
- **Mesh announcements** need nobody to say anything — the speaker pipes up on its
  own when a mesh message arrives.

---

## 1. Bitcoin — block height (local, instant)

- "what's the block height"
- "what's the current block height"
- "what is the block height"
- "block height"
- "current block height"
- "how many blocks"
- "how many blocks are there"

## 2. Peers — bitcoin **and** mesh in one answer (local, instant)

The answer includes both your bitcoin peer count and your mesh peer count.

- "how many peers"
- "how many peers do I have"
- "how many peers is the node connected to"
- "peer count"
- "node peer count"

## 3. Sync status (local, instant)

Fully synced → it tells you the block it's synced to. Still syncing → the percent.

- "is the node synced"
- "is bitcoin synced"
- "is the node fully synced"
- "is bitcoin fully synced"
- "how synced is the node"
- "how synced is bitcoin"
- "sync status"
- "bitcoin sync status"
- "sync progress"
- "sync percentage"

## 4. Lightning balance (local, instant)

- "what's my lightning balance"
- "what is my lightning balance"
- "lightning balance"
- "how many sats do I have"
- "how many sats are in my wallet"

---

## 5. Same questions, said like a human (Claude routes to the node)

The point of the AI brain: you don't have to remember the magic words. All of
these end in the same real numbers as sections 1–4:

- "how tall is the chain right now"
- "what block are we on"
- "is my bitcoin thing done downloading yet"
- "how far along is the sync"
- "are we caught up with the blockchain"
- "how's my node doing"
- "is everything okay with the node"
- "how many people is my node talking to"
- "is anyone connected to my node"
- "am I rich in lightning"
- "how much money is in my lightning wallet"
- "do I have any sats"
- "what's my node's status"

## 6. Mesh

**Hands-free announcements** — when a mesh text arrives, the speaker announces it
by itself: *"New mesh message from ⟨sender⟩: ⟨text⟩"*. Nothing to say; just have
someone send you one.

**Asking about the mesh:**

- "how many peers" — the local answer already includes mesh peers
- "how many mesh peers do I have"
- "am I connected to the mesh"
- "what was the last mesh message"
- "who sent the last mesh message"
- "read me the latest mesh message"
- "did I get any mesh messages"

(The last-message questions go through Claude reading the mesh sensor — phrasing
is free-form.)

## 7. Ask the AI anything

Short spoken answers, one or two sentences, no robot-reading-markdown. A sampler
by mood:

**Bitcoin & lightning, explained**
- "what actually happens when a block is mined"
- "explain the halving like I'm five"
- "what's the difference between on-chain and lightning"
- "why do confirmations matter"
- "what's a mempool"
- "is it normal for sync to take days"

**Everyday brain**
- "why is the sky blue"
- "how long do I boil an egg"
- "what can I cook with eggs and spinach"
- "what's 15 percent of 84"
- "how many ounces in a kilo"
- "what's 21 million divided by 8 billion"
- "how do you say good morning in Portuguese"
- "give me a word that rhymes with orange"

**Fun**
- "tell me a joke"
- "tell me a bitcoin joke"
- "give me a fun fact"
- "tell me a two-sentence scary story"
- "settle an argument: is a hotdog a sandwich"

**Advice-ish**
- "what should I name my node"
- "give me one tip for keeping my seed phrase safe"
- "what's a good way to explain my node to my mum"

Follow-ups work conversationally — ask something, then "and why is that?" without
re-explaining yourself.

## 8. Built-in assistant basics (Home Assistant, local)

- "what time is it"
- "what's the date today"
- "set a timer for 5 minutes" / "cancel the timer" / "how long is left on the
  timer" — *timer support depends on the PineVoice satellite build; try it once
  and you'll know.*
- "nevermind" / "cancel" — bail out of a listening session.

## 9. Smart home (only if you've got devices in Home Assistant)

Pine rides on Home Assistant Assist, so if you ever add exposed devices
(lights, plugs, sensors), the standard grammar lights up automatically:

- "turn on the living room light" / "turn off everything"
- "is the front door locked"
- "what's the temperature inside"

No devices → these politely fail; nothing to test today.

## 10. Known not-to-work (yet) — don't burn time on these

- **Sending** a mesh message by voice ("tell Bob I'm on my way") — receive/announce
  only, for now.
- Controlling the node by voice ("restart bitcoin", "install an app") — read-only
  on purpose.
- Long memory across sessions — each conversation is fresh.

---

## If something misbehaves

Say exactly what you said, what it answered (or didn't), and roughly when — the
node keeps logs of every pipeline run and it's usually a one-look diagnosis.
