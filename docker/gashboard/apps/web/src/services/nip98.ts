import type { EventTemplate } from "nostr-tools";
import { getActiveSigner } from "./signer";

export async function buildNip98AuthHeader(method: string, fullUrl: string): Promise<string> {
  const signer = getActiveSigner();
  if (!signer) throw new Error("Not logged in");

  const template: EventTemplate = {
    kind: 27235,
    created_at: Math.floor(Date.now() / 1000),
    tags: [
      ["u", fullUrl],
      ["method", method.toUpperCase()],
    ],
    content: "",
  };
  const signed = await signer.signEvent(template);
  const json = JSON.stringify(signed);
  const b64 = btoa(unescape(encodeURIComponent(json)));
  return `Nostr ${b64}`;
}
