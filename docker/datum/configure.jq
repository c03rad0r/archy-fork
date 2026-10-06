if type != "object" then error("Datum config must be an object") else . end
| .bitcoind.rpcurl = ("http://" + env.BITCOIN_RPC_HOST + ":8332")
| .bitcoind.rpcuser = "archipelago"
| .bitcoind.rpcpassword = env.BITCOIN_RPC_PASSWORD
# Use upstream's getbestblockhash fallback; no host bitcoind hooks needed.
| .bitcoind.notify_fallback = true
| .stratum.listen_addr = "0.0.0.0"
| .stratum.listen_port = 23334
| .mining.pool_address //= ""
| .mining.coinbase_tag_primary //= "DATUM Gateway"
| .mining.coinbase_tag_secondary //= "Archipelago"
| .api.listen_port = 7152
| .api.admin_password = env.DATUM_ADMIN_PASSWORD
| .api.modify_conf = true
| .logger.log_to_console = true
| .logger.log_to_file = false
| if .datum.pool_pass_workers == null then .datum.pool_pass_workers = true else . end
| if .datum.pool_pass_full_users == null then .datum.pool_pass_full_users = true else . end
| if .datum.pooled_mining_only == null then .datum.pooled_mining_only = true else . end
