package com.archipelago.app.nostr

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import com.archipelago.app.NativeCore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

private val Context.signerDataStore: DataStore<Preferences> by preferencesDataStore(name = "nostr_signer")

/**
 * Storage for the phone-side NIP-46 remote signer (#139): the signer secret
 * key (hex) and the last pairing, so a re-opened app can resume a session
 * without re-scanning the node's QR.
 *
 * Same plaintext-DataStore model as the FIPS secret (app-private storage,
 * no extra OS keystore ceremony — the node login password lives the same way
 * in ServerPreferences); the nsec grants the ability to sign as this identity,
 * never node login.
 */
class NostrSignerPreferences(private val context: Context) {

    private val secretKey = stringPreferencesKey("signer_secret")
    private val clientPubkeyKey = stringPreferencesKey("pair_client_pubkey")
    private val clientRelayKey = stringPreferencesKey("pair_client_relay")
    private val clientNameKey = stringPreferencesKey("pair_client_name")

    /** The signer secret (hex) or null when no key exists yet. */
    suspend fun secret(): String? = context.signerDataStore.data.first()[secretKey]

    val secretFlow: Flow<String?> = context.signerDataStore.data
        .map { it[secretKey] }
        .distinctUntilChanged()

    suspend fun saveSecret(hex: String) {
        context.signerDataStore.edit { it[secretKey] = hex.trim() }
    }

    /** Generate a fresh signer key (fails if the native core is missing). */
    suspend fun generateSecret(): JSONObject = withContext(Dispatchers.IO) {
        val json = NativeCore.nostrGenerateSecret()
        val obj = JSONObject(json)
        if (obj.has("error")) throw IllegalStateException(obj.getString("error"))
        saveSecret(obj.getString("secret"))
        obj
    }

    /** Import a secret from hex or nsec…; returns the parsed key info. */
    suspend fun importSecret(raw: String): JSONObject = withContext(Dispatchers.IO) {
        val json = NativeCore.nostrSecretFromAny(raw.trim())
        val obj = JSONObject(json)
        if (obj.has("error")) throw IllegalArgumentException(obj.getString("error"))
        saveSecret(obj.getString("secret"))
        obj
    }

    data class Pairing(val clientPubkey: String, val relay: String, val name: String)

    suspend fun lastPairing(): Pairing? {
        val prefs = context.signerDataStore.data.first()
        val pubkey = prefs[clientPubkeyKey] ?: return null
        val relay = prefs[clientRelayKey] ?: return null
        if (pubkey.isBlank() || relay.isBlank()) return null
        return Pairing(pubkey, relay, prefs[clientNameKey] ?: "")
    }

    suspend fun savePairing(pairing: Pairing) {
        context.signerDataStore.edit {
            it[clientPubkeyKey] = pairing.clientPubkey
            it[clientRelayKey] = pairing.relay
            it[clientNameKey] = pairing.name
        }
    }

    suspend fun clearPairing() {
        context.signerDataStore.edit {
            it.remove(clientPubkeyKey)
            it.remove(clientRelayKey)
            it.remove(clientNameKey)
        }
    }

    suspend fun wipeKey() {
        context.signerDataStore.edit { it.remove(secretKey) }
    }
}
