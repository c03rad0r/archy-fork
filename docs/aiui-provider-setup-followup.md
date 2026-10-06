# AIUI provider setup follow-up

Status: implementation in progress; not deployed or accepted.

The app's browser key vault did not configure the node's authoritative Claude
ledger. Embedded chat delegates to the node's tool loop, whose provider selection
also ignored the frontend's Claude/OpenRouter picker. The backend retired the
OpenRouter relay while that picker still offered it. Generic502/503 errors were
classified as missing keys and sent users back to settings.

Implementation scope: offer setup in trusted dashboard chrome before first use;
keep credentials out of the iframe's chat, prompts, history and browser storage;
use private atomic node credential writes; persist an explicit provider choice;
retain the existing tool permissions and outbound privacy screen. Explicit
provider selection must not silently send a failed request to a different cloud
provider. Routstr funding and allowance remain separate, deliberate actions.

OpenAI support uses its standard API key, not a presumed Codex subscription key.
For the existing node tool loop, the Chat Completions API retains the same
message/tool-result representation and existing egress checks. This is an
intentional integration choice, not a claim that it is the newer Responses API.
The HTTP adapter has a fixed HTTPS destination, no redirects or retries, a bounded
completion and response, store:false, and errors that do not echo upstream bodies.
The operator supplies the model ID; no inference is issued merely by saving a key.

Official documentation checked2026-10-06:
- https://developers.openai.com/api/reference/overview (server-side bearer credentials)
- https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create
  (max_completion_tokens, tool calls, store)
- https://developers.openai.com/api/docs/guides/streaming-responses
  (Responses recommendation and distinction from Chat Completions)

Qualification so far: all 1,244 dashboard tests and 363 AIUI tests pass before
final toolbar placement/funding refinements; latest focused checks pass 15
dashboard and 24 AIUI tests. Both production bundles build. Chromium390/1440px
verifies first-use setup, private fixture key/model save, no key in localStorage,
retained unsent draft and no page errors. Visual review found an overlapping
mobile setup button; moved setup into the existing model menu. That final layout
still needs rebuilt-browser qualification. No fixture key reached a real provider.

Explicit local selection now stays local; Claude/OpenAI selection cannot silently
fall through. Routstr selection persists and retains the existing budget checks.
Payment-required responses request the funding view, while temporary502/503 and
rate limits do not claim credentials are missing. Reopening ecash funding reloads
the address, including repeated opens on the same tab.

Remaining: isolated backend results, final browser/funding/key-error checks,
actual provider compatibility and node deployment. No paid inference tests or new
wallet spending have been performed or authorized by this implementation work.
