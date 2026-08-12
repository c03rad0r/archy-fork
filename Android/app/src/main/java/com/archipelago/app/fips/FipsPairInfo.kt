package com.archipelago.app.fips

/**
 * Node mesh parameters carried by the pairing QR (fnpub/fip/fhost/fudp/ftcp —
 * docs/companion-pairing-qr.md). The phone's embedded FIPS node dials
 * host:udpPort / host:tcpPort claiming nothing; the mesh accepts inbound peers
 * without registration, so possession of these params is all pairing takes.
 */
data class FipsPairInfo(
    val npub: String,
    /** Node's fips0 ULA — where its UI stays reachable once meshed. May be empty. */
    val ula: String,
    val host: String,
    val udpPort: Int,
    val tcpPort: Int,
    /**
     * Public rendezvous anchors (the node's seed-anchor list). The phone
     * peers with these too, so it can route to the node via the mesh when
     * the node's LAN endpoint isn't directly dialable.
     */
    val anchors: List<AnchorPeer> = emptyList(),
)

/** One rendezvous anchor from the QR's `fanchors` param (npub@addr/transport). */
data class AnchorPeer(
    val npub: String,
    val addr: String,
    val transport: String,
)
