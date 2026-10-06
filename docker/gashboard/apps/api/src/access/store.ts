import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { nip19 } from "nostr-tools";

export class AccessList {
  private readonly owners: Set<string>;
  private viewers: Set<string>;

  constructor(private readonly file: string, owners: string[], initialViewers: string[]) {
    this.owners = new Set(owners);
    try {
      const saved: unknown = JSON.parse(readFileSync(file, "utf8"));
      if (!Array.isArray(saved) || saved.length > 500 || saved.some(k => typeof k !== "string" || !/^[0-9a-f]{64}$/.test(k))) {
        throw new Error("Invalid saved Gashboard access list");
      }
      this.viewers = new Set(saved as string[]);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error;
      this.viewers = new Set(initialViewers);
    }
  }

  isOwner(pubkey: string): boolean { return this.owners.has(pubkey); }
  isAllowed(pubkey: string): boolean { return this.isOwner(pubkey) || this.viewers.has(pubkey); }
  keys(): string[] { return [...new Set([...this.owners, ...this.viewers])].sort(); }
  members(): Array<{ npub: string; role: "owner" | "viewer" }> {
    return this.keys().map(pubkey => ({ npub: nip19.npubEncode(pubkey), role: this.isOwner(pubkey) ? "owner" : "viewer" }));
  }

  setViewer(pubkey: string, enabled: boolean): void {
    if (this.isOwner(pubkey)) throw new Error("Node owners are managed in Archipelago identities");
    const next = new Set(this.viewers);
    if (enabled) next.add(pubkey); else next.delete(pubkey);
    if (next.size > 500) throw new Error("The access list is limited to 500 viewers");
    mkdirSync(dirname(this.file), { recursive: true, mode: 0o700 });
    // Small synchronous writes serialize mutations; replace before updating memory.
    writeFileSync(`${this.file}.tmp`, JSON.stringify([...next].sort()) + "\n", { mode: 0o600 });
    renameSync(`${this.file}.tmp`, this.file);
    this.viewers = next;
  }
}
