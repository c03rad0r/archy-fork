import { config } from "../config.js";
import { AccessList } from "../access/store.js";

export const accessList = new AccessList(config.nostr.accessListPath, config.nostr.ownerHexPubkeys, config.nostr.allowedHexPubkeys);
export const isPubkeyAllowed = (pubkey: string): boolean => accessList.isAllowed(pubkey);
export const isOwner = (pubkey: string): boolean => accessList.isOwner(pubkey);
export const allowedPubkeys = (): string[] => accessList.keys();
