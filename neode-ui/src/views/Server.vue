<template>
  <div class="pb-6">

    <!-- Subtle background-refresh indicator (D-05): renders nothing unless
         any of the seven cached load groups is revalidating. Additive-only —
         no existing element/selector is touched. -->
    <div class="flex justify-end">
      <RefreshIndicator :state="serverRefreshIndicatorState" label="Refreshing server status" />
    </div>

    <!-- LUKS Encryption Badge -->
    <div v-if="diskEncrypted" class="mb-4 px-4 py-2.5 rounded-xl border bg-green-500/5 border-green-500/20 flex items-center gap-2.5">
      <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-green-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
      <span class="text-xs text-green-300/80 font-medium">LUKS2 Encrypted Storage</span>
    </div>

    <!-- Disk Space Warning Banner -->
    <div
      v-if="diskWarning"
      class="mb-6 p-4 rounded-xl border flex items-center justify-between"
      :class="diskWarning.level === 'critical'
        ? 'bg-red-500/10 border-red-500/30'
        : 'bg-yellow-500/10 border-yellow-500/30'"
    >
      <div class="flex items-center gap-3">
        <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5 shrink-0" :class="diskWarning.level === 'critical' ? 'text-red-400' : 'text-yellow-400'" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <div>
          <p class="text-sm font-medium" :class="diskWarning.level === 'critical' ? 'text-red-300' : 'text-yellow-300'">
            {{ diskWarning.level === 'critical' ? 'Disk Space Critical' : 'Disk Space Warning' }}
          </p>
          <p class="text-xs text-white/60">
            {{ diskWarning.used_percent.toFixed(1) }}% used — {{ formatBytes(diskWarning.free_bytes) }} remaining
          </p>
        </div>
      </div>
      <button
        class="glass-button glass-button-sm px-3 py-1.5 text-xs font-medium rounded"
        :disabled="diskCleaning"
        @click="runDiskCleanup"
      >
        {{ diskCleaning ? 'Cleaning...' : 'Clean Up' }}
      </button>
    </div>

    <!-- Quick Actions -->
    <QuickActionsCard
      :services-running="servicesRunning"
      :restarting="restarting"
      :tor-status-label="torStatusLabel"
      :tor-status-color="torStatusColor"
      :checking-tor="checkingTor"
      :auto-sync-enabled="autoSyncEnabled"
      :log-count="logCount"
      @restart-services="restartServices"
      @check-tor="checkTorStatus"
      @update:auto-sync-enabled="autoSyncEnabled = $event"
      @view-logs="viewLogs"
    />

    <!-- Overview Cards -->
    <div class="grid grid-cols-1 xl:grid-cols-2 gap-6 mb-8">
      <!-- Local Network Card -->
      <div data-controller-container tabindex="0" class="glass-card p-6 flex flex-col transition-all hover:-translate-y-1">
        <div class="flex items-start gap-4 mb-4 shrink-0">
          <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
            <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
            </svg>
          </div>
          <div class="flex-1">
            <h2 class="text-xl font-semibold text-white mb-2">Local Network</h2>
            <p class="text-white/70 text-sm mb-4">OpenWRT-integrated router and network management</p>
          </div>
        </div>

        <div class="space-y-3 flex-1 min-h-0">
          <template v-if="networkLoading">
            <div v-for="i in 4" :key="i" class="flex items-center justify-between p-3 bg-white/5 rounded-lg animate-pulse">
              <div class="flex items-center gap-3">
                <div class="w-5 h-5 bg-white/10 rounded"></div>
                <div class="w-24 h-4 bg-white/10 rounded"></div>
              </div>
              <div class="w-16 h-4 bg-white/10 rounded"></div>
            </div>
          </template>

          <template v-else>
            <div v-if="networkRefreshing" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
              <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Refreshing network...
            </div>
            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" /></svg>
                <span class="text-white/80 text-sm">Firewall Active</span>
              </div>
              <span class="text-green-400 text-sm font-medium">Protected</span>
            </div>
            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" /></svg>
                <span class="text-white/80 text-sm">WiFi</span>
              </div>
              <span class="text-sm" :class="networkData.wifiSsid ? 'text-green-400' : 'text-white/40'">{{ networkData.wifiSsid || 'Not connected' }}</span>
            </div>
            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" /></svg>
                <span class="text-white/80 text-sm">Tor</span>
              </div>
              <span class="text-sm" :class="torStatusLabel === 'running' ? 'text-green-400' : 'text-white/60'">{{ torStatusLabel === 'running' ? 'Connected' : torStatusLabel === 'checking' ? 'Checking...' : 'Stopped' }}</span>
            </div>
            <router-link
              to="/dashboard/server/openwrt"
              class="w-full flex items-center justify-between p-3 bg-white/5 rounded-lg hover:bg-white/10 transition-colors text-left"
            >
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" /></svg>
                <span class="text-white/80 text-sm">OpenWrt Gateway</span>
              </div>
              <svg class="w-4 h-4 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" /></svg>
            </router-link>
            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
                <span class="text-white/80 text-sm">Port Forwarding</span>
              </div>
              <span class="text-white/60 text-sm">{{ networkData.forwardCount }}</span>
            </div>
            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
                <span class="text-white/80 text-sm">VPN</span>
              </div>
              <span class="text-sm" :class="networkData.vpnConnected ? 'text-green-400' : 'text-white/40'">
                {{ networkData.vpnConnected ? 'WireGuard' : 'Not Connected' }}
              </span>
            </div>
            <button class="w-full flex items-center justify-between p-3 bg-white/5 rounded-lg hover:bg-white/10 transition-colors text-left" @click="showDnsModal = true">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9" /></svg>
                <span class="text-white/80 text-sm">DNS</span>
              </div>
              <span class="text-sm" :class="networkData.dnsProvider !== 'system' ? 'text-green-400' : 'text-white/60'">
                {{ dnsDisplayLabel }}
              </span>
            </button>
            <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
                <span class="text-white/80 text-sm">F*ck IPs Mesh</span>
              </div>
              <span class="text-sm" :class="fipsRowTextClass">{{ fipsRowLabel }}</span>
            </div>
          </template>
        </div>
      </div>

      <FipsNetworkCard />
    </div>

    <div class="grid grid-cols-1 xl:grid-cols-2 gap-6 mb-6">
    <!-- VPN Card -->
    <div class="glass-card p-6 flex flex-col transition-all hover:-translate-y-1">
      <div class="flex items-center justify-between mb-4">
        <div class="flex items-center gap-3">
          <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
            <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" /></svg>
          </div>
          <div>
            <h2 class="text-lg font-semibold text-white">VPN</h2>
            <p class="text-xs text-white/50">Standalone WireGuard VPN</p>
          </div>
        </div>
        <button @click="showAddDeviceModal = true; showingNewDevice = true" class="responsive-card-actions-top glass-button px-4 py-2 text-sm">Add Device</button>
      </div>

      <!-- WireGuard Status -->
      <div class="mb-4 p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-2 mb-1">
          <div class="w-2 h-2 rounded-full" :class="networkData.wgIp ? 'bg-green-400' : 'bg-white/20'"></div>
          <span class="text-xs text-white/50">Server Address</span>
        </div>
        <span class="text-sm font-mono" :class="networkData.wgIp ? 'text-white' : 'text-white/30'">{{ networkData.wgIp || 'Not configured' }}</span>
        <span v-if="networkData.wgPubkey" class="block text-xs font-mono text-white/30 mt-1 truncate">{{ networkData.wgPubkey }}</span>
      </div>

      <!-- Connected Devices -->
      <div class="border-t border-white/10 pt-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs text-white/50">Connected Devices</span>
          <span class="text-xs text-white/30">{{ vpnPeers.length }} device{{ vpnPeers.length !== 1 ? 's' : '' }}</span>
        </div>
        <div v-if="vpnPeers.length" class="space-y-1">
          <div v-for="peer in vpnPeers" :key="peer.name" class="flex items-center justify-between text-xs py-1.5 px-2 bg-white/5 rounded">
            <div class="flex items-center gap-2">
              <span class="px-1 py-0.5 rounded text-[10px] font-medium bg-blue-500/20 text-blue-300">WG</span>
              <button @click="showPeerConfig(peer.name)" class="text-white/70 hover:text-white transition-colors cursor-pointer">{{ peer.name }}</button>
            </div>
            <div class="flex items-center gap-2">
              <span class="text-white/40 font-mono">{{ peer.ip?.replace(/\/\d+$/, '') || '' }}</span>
              <button @click="removePeer(peer.name)" :disabled="removingPeer === peer.name" class="p-0.5 rounded hover:bg-white/10 text-white/30 hover:text-red-400 transition-colors" :title="'Remove ' + peer.name">
                <svg v-if="removingPeer === peer.name" class="w-3.5 h-3.5 animate-spin" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" /><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>
                <svg v-else class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
              </button>
            </div>
          </div>
        </div>
        <div v-else class="text-xs text-white/30 py-2">No devices added yet</div>
      </div>

      <!-- mt-auto pins the action to the card bottom so buttons align across
           equal-height grid cards -->
      <div class="responsive-card-actions-bottom mt-auto pt-4">
        <button @click="showAddDeviceModal = true; showingNewDevice = true" class="mobile-card-action glass-button rounded-lg text-sm font-medium">
          Add Device
        </button>
      </div>
    </div>

    <!-- Network Interfaces (second column on desktop) -->
    <div data-controller-container tabindex="0" class="glass-card p-6 flex flex-col transition-all hover:-translate-y-1">
      <div class="flex items-center justify-between mb-4">
        <div>
          <h2 class="text-xl font-semibold text-white mb-1">Network Interfaces</h2>
          <p class="text-sm text-white/60">Detected hardware and virtual interfaces</p>
        </div>
        <button
          v-if="wifiAvailable"
          @click="showWifiModal = true"
          class="responsive-card-actions-top px-3 py-1.5 glass-button rounded text-xs font-medium text-white/90 hover:text-white transition-colors"
        >
          Scan WiFi
        </button>
      </div>

      <template v-if="interfacesLoading">
        <div class="space-y-3">
          <div v-for="i in 3" :key="i" class="p-3 bg-white/5 rounded-lg animate-pulse h-14"></div>
        </div>
      </template>
      <template v-else>
        <div class="space-y-3">
          <div v-if="interfacesRefreshing" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
            <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            Refreshing interfaces...
          </div>
          <div
            v-for="iface in physicalInterfaces"
            :key="iface.name"
            class="flex items-center justify-between p-3 bg-white/5 rounded-lg"
          >
            <div class="flex items-center gap-3">
              <div class="w-2 h-2 rounded-full" :class="iface.state === 'up' ? 'bg-green-400' : 'bg-white/30'"></div>
              <div>
                <p class="text-sm text-white font-medium">{{ iface.name }}</p>
                <p class="text-xs text-white/50">{{ iface.type === 'wifi' ? 'WiFi' : 'Ethernet' }} &middot; {{ iface.mac }}</p>
              </div>
            </div>
            <div class="flex items-center gap-3">
              <div class="text-right">
                <p v-if="iface.ipv4.length > 0" class="text-sm text-white/80">{{ iface.ipv4[0] }}</p>
                <p v-else class="text-sm text-white/40">No IP</p>
              </div>
              <ToggleSwitch
                v-if="iface.type === 'wifi'"
                :model-value="iface.state === 'up'"
                :disabled="togglingWifiRadio"
                :aria-label="iface.state === 'up' ? 'Turn off wifi adapter' : 'Turn on wifi adapter'"
                @update:model-value="toggleWifiRadio(iface)"
              />
            </div>
          </div>
          <div v-if="interfacesError" role="status" class="flex items-center justify-between gap-3 text-sm text-white/60 py-2">
            <span>{{ allInterfaces.length ? 'Couldn’t refresh interfaces. Showing last known information.' : 'Couldn’t load network interfaces.' }}</span>
            <button class="glass-button rounded px-3 py-1.5 text-xs shrink-0" :disabled="interfacesRefreshing" @click="loadInterfaces">Retry</button>
          </div>
          <p v-else-if="physicalInterfaces.length === 0 && interfacesRes.data.value !== null" class="text-sm text-white/50 text-center py-4">No physical interfaces detected</p>
          <p v-if="wifiRadioError" class="text-xs text-red-400">{{ wifiRadioError }}</p>
        </div>
      </template>

      <div v-if="wifiAvailable" class="responsive-card-actions-bottom mt-auto pt-4">
        <button
          @click="showWifiModal = true"
          class="mobile-card-action glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors"
        >
          Scan WiFi
        </button>
      </div>
    </div>

    </div><!-- close VPN+Network 2-col grid -->

    <!-- Add Device Modal -->
    <Teleport to="body">
      <Transition name="modal">
        <div v-if="showAddDeviceModal" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click="closeDeviceModal">
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
          <div @click.stop class="glass-card p-6 max-w-md w-full relative z-10">
            <div class="flex items-center justify-between mb-4">
              <h3 class="text-lg font-semibold text-white">Connect Device</h3>
              <button @click="closeDeviceModal" class="p-1 rounded hover:bg-white/10 text-white/60"><svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg></button>
            </div>
            <!-- Loading state (for existing peer config) -->
            <div v-if="loadingPeerConfig" class="text-center py-8">
              <svg class="w-6 h-6 animate-spin text-white/40 mx-auto" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" /><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>
            </div>
            <!-- Existing peer QR view -->
            <div v-else-if="peerQrData && !showingNewDevice" class="text-center">
              <div class="bg-white rounded-xl p-4 mb-4 inline-block" v-html="sanitizedPeerQrSvg"></div>
              <p class="text-sm text-white/70 mb-2">Scan with the <strong>WireGuard</strong> app</p>
              <p class="text-xs text-white/40 font-mono mb-4">{{ peerQrData.peer_ip }}</p>
              <div class="flex gap-2">
                <button @click="copyPeerConfig" class="flex-1 glass-button py-2 text-xs">{{ copiedConfig ? 'Copied!' : 'Copy Config' }}</button>
                <button @click="closeDeviceModal" class="flex-1 glass-button py-2 text-xs">Done</button>
              </div>
            </div>
            <!-- New device: WireGuard config -->
            <div v-else>
              <div v-if="peerQrData">
                <div class="text-center">
                  <div class="bg-white rounded-xl p-4 mb-4 inline-block" v-html="sanitizedPeerQrSvg"></div>
                  <p class="text-sm text-white/70 mb-2">Scan with the <strong>WireGuard</strong> app</p>
                  <p class="text-xs text-white/40 font-mono mb-4">{{ peerQrData.peer_ip }}</p>
                  <div class="flex gap-2">
                    <button @click="copyPeerConfig" class="flex-1 glass-button py-2 text-xs">{{ copiedConfig ? 'Copied!' : 'Copy Config' }}</button>
                    <button @click="closeDeviceModal" class="flex-1 glass-button py-2 text-xs">Done</button>
                  </div>
                </div>
              </div>
              <div v-else>
                <div>
                  <p class="text-sm text-white/50 mb-3">Generate a WireGuard config for the standard WireGuard app.</p>
                  <input v-model="newPeerName" type="text" placeholder="Device name (e.g. iPhone)" class="w-full bg-white/5 border border-white/10 rounded-lg px-4 py-2.5 text-sm text-white placeholder-white/30 focus:outline-none focus:border-white/30 mb-3" @keyup.enter="createPeer" />
                  <button @click="createPeer" :disabled="creatingPeer || !newPeerName.trim()" class="w-full glass-button py-2.5 text-sm font-medium disabled:opacity-30">{{ creatingPeer ? 'Generating...' : 'Generate QR Code' }}</button>
                </div>
              </div>
            </div>
            <p v-if="peerError" class="text-sm text-red-400 mt-2">{{ peerError }}</p>
          </div>
        </div>
      </Transition>
    </Teleport>

    <div class="mb-6">
      <!-- Tor Services -->
      <TorServicesCard
        :tor-services="torServices"
        :tor-services-loading="torServicesLoading"
        :tor-daemon-running="torDaemonRunning"
        :tor-restarting="torRestarting"
        :tor-rotating="torRotating"
        :tor-deleting="torDeleting"
        @restart-tor="restartTor"
        @show-add-service="showAddServiceModal = true"
        @copy-address="copyTorAddress"
        @rotate-service="rotateService"
        @delete-service="deleteService"
        @toggle-app="toggleTorApp"
      />
    </div>

    <!-- Modals -->
    <ServerModals
      :show-add-service-modal="showAddServiceModal"
      :show-wifi-modal="showWifiModal"
      :show-dns-modal="showDnsModal"
      :available-apps-for-tor="availableAppsForTor"
      :adding-service="addingService"
      :add-service-error="addServiceError"
      :wifi-scanning="wifiScanning"
      :wifi-networks="wifiNetworks"
      :wifi-connecting="wifiConnecting"
      :wifi-submitting="wifiSubmitting"
      :wifi-selected-ssid="wifiSelectedSsid"
      :wifi-error="wifiError"
      :wifi-scan-error="wifiScanError"
      :dns-selected-provider="dnsSelectedProvider"
      :dns-servers="networkData.dnsServers"
      :dns-applying="dnsApplying"
      :dns-error="dnsError"
      :dns-provider-options="dnsProviderOptions"
      @close-add-service="showAddServiceModal = false"
      @create-service-for-app="createServiceForApp"
      @create-service="createService"
      @close-wifi="showWifiModal = false"
      @select-wifi="selectWifi"
      @connect-wifi="connectToWifi"
      @scan-wifi="scanWifi"
      @cancel-wifi-connect="wifiConnecting = false; wifiPassword = ''; wifiError = ''"
      @close-dns="showDnsModal = false; dnsError = ''"
      @select-dns-provider="(v: string) => { dnsSelectedProvider = v }"
      @apply-dns="applyDnsConfig"
    />

    <!-- Logs info toast -->
    <Transition name="fade">
      <div v-if="logsToast" class="fixed bottom-20 left-1/2 -translate-x-1/2 z-50 max-w-md w-full px-4">
        <div class="bg-white/10 border border-white/20 backdrop-blur-sm rounded-lg px-4 py-3 text-white/80 text-sm flex items-center justify-between gap-3">
          <span>{{ logsToast }}</span>
          <button @click="logsToast = ''" class="text-white/50 hover:text-white shrink-0">&times;</button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onActivated, onDeactivated, onMounted, onUnmounted, watch } from 'vue'
import DOMPurify from 'dompurify'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource, type CachedResource } from '@/composables/useCachedResource'
import RefreshIndicator from '@/components/RefreshIndicator.vue'
import { useAppStore } from '@/stores/app'
import QuickActionsCard from './server/QuickActionsCard.vue'
import TorServicesCard from './server/TorServicesCard.vue'
import ServerModals from './server/ServerModals.vue'
import FipsNetworkCard from './server/FipsNetworkCard.vue'
import ToggleSwitch from '@/components/ToggleSwitch.vue'
import type { TorServiceInfo } from './server/TorServicesCard.vue'

const appStore = useAppStore()

// Service status
const servicesRunning = ref(true)
const restarting = ref(false)

// Tor status
const torStatusLabel = ref<'running' | 'stopped' | 'checking'>('checking')
const checkingTor = ref(false)
const torStatusColor = computed(() => {
  if (torStatusLabel.value === 'running') return 'bg-green-400'
  if (torStatusLabel.value === 'checking') return 'bg-yellow-400'
  return 'bg-red-400'
})

// Auto-sync, logs
const autoSyncEnabled = ref(true)
const logCount = ref(0)

// Network data — a cached aggregate over four RPCs (allSettled: a failing
// one keeps that slice's previous values). Revisits paint instantly.
interface NetworkData {
  wifiCount: string; wifiSsid: string | null; torConnected: boolean; forwardCount: string
  vpnConnected: boolean; vpnProvider: string; vpnIp: string; wgIp: string; wgPubkey: string
  vpnHostname: string; vpnPeers: number
  dnsProvider: string; dnsServers: string[]; dnsDoH: boolean
}
const defaultNetworkData = (): NetworkData => ({
  wifiCount: 'N/A', wifiSsid: null, torConnected: false, forwardCount: 'N/A',
  vpnConnected: false, vpnProvider: '', vpnIp: '', wgIp: '', wgPubkey: '', vpnHostname: '', vpnPeers: 0,
  dnsProvider: 'system', dnsServers: [], dnsDoH: false,
})
// immediate:false — the fetcher merges onto the previous value via
// networkRes, so it must not run during this initializer (onMounted loads
// it). The explicit annotation breaks the self-referential inference cycle.
//
// TTL/persist (02-06, T-02-01/D-06): 10s — this bucket (network/interfaces/
// disk) moves fast enough (WiFi/VPN/DNS state) to warrant the short tier.
// persist:true — this is this node's OWN aggregate network/VPN-connection
// status (not another peer's identity); wgPubkey here is the local node's
// own WireGuard public key, already surfaced in this same UI, not a peer
// identity payload — distinct from server.vpn-peers below, which does carry
// other peers' identity and is persist:false.
const networkRes: CachedResource<NetworkData> = useCachedResource<NetworkData>({
  key: 'server.network-summary',
  immediate: false,
  ttlMs: 10_000,
  persist: true,
  fetcher: async (signal) => {
    const next = { ...(networkRes.data.value ?? defaultNetworkData()) }
    const [diagRes, fwdRes, vpnRes, dnsRes] = await Promise.allSettled([
      rpcClient.call<{ wan_ip: string | null; nat_type: string; upnp_available: boolean; tor_connected: boolean; wifi_count?: number }>({ method: 'network.diagnostics', signal, dedup: true, maxRetries: 1 }),
      rpcClient.call<{ forwards: unknown[] }>({ method: 'router.list-forwards', signal, dedup: true, maxRetries: 1 }),
      rpcClient.vpnStatus(signal),
      rpcClient.dnsStatus(signal),
    ])
    if (diagRes.status === 'fulfilled') { next.torConnected = diagRes.value.tor_connected; next.wifiCount = diagRes.value.wifi_count !== undefined ? `${diagRes.value.wifi_count} configured` : 'N/A'; next.wifiSsid = (diagRes.value as { wifi_ssid?: string | null }).wifi_ssid ?? null }
    if (fwdRes.status === 'fulfilled') { const c = fwdRes.value.forwards?.length ?? 0; next.forwardCount = `${c} rule${c !== 1 ? 's' : ''}` }
    if (vpnRes.status === 'fulfilled') { next.vpnConnected = vpnRes.value.connected; next.vpnProvider = vpnRes.value.provider ?? ''; next.vpnIp = (vpnRes.value.ip_address ?? '').replace(/\/\d+$/, ''); next.wgIp = vpnRes.value.wg_ip ?? ''; next.wgPubkey = (vpnRes.value as Record<string, unknown>).wg_pubkey as string ?? '' }
    if (dnsRes.status === 'fulfilled') { next.dnsProvider = dnsRes.value.provider; next.dnsServers = dnsRes.value.resolv_conf_servers ?? []; next.dnsDoH = dnsRes.value.doh_enabled }
    return next
  },
})
const networkData = computed(() => networkRes.data.value ?? defaultNetworkData())
const networkLoading = computed(() => networkRes.loadState.value === 'loading')
const networkRefreshing = computed(() => networkRes.loadState.value === 'refreshing')

// FIPS status row for the Local Network card. Full FIPS card lives below.
// TTL 60s (near-static: installed/service_active/key_present rarely change,
// but authenticated_peer_count can drift, so this sits between the fast
// 10s tier and Tor/VPN's 30s tier rather than a fully-static value).
// persist:false (CR-01 follow-up audit correction) — this component's own
// local type only names installed/service_active/key_present/etc, but
// `fips.status` is the SAME RPC/cache key FipsNetworkCard.vue shares
// ("Shares `server.fips-summary`..."), and THAT component's fuller
// `FipsStatus` type shows the real response also carries `npub` — this
// node's own FIPS identity public key. Whichever of the two fetchers wins a
// given refresh writes the FULL raw response to sessionStorage regardless
// of either caller's narrower TS type, so this key must follow the same
// "own identity — never persist" rule as mesh.self-onion/mesh.self-did
// (T-02-01), not the "aggregate status only" rule its old comment assumed.
const fipsSummaryRes = useCachedResource<{ installed: boolean; service_active: boolean; key_present: boolean; anchor_connected?: boolean; authenticated_peer_count?: number }>({
  key: 'server.fips-summary',
  immediate: false,
  ttlMs: 60_000,
  persist: false,
  fetcher: (signal) => rpcClient.call({ method: 'fips.status', signal, dedup: true, maxRetries: 1 }),
})
const fipsSummary = computed(() => fipsSummaryRes.data.value)
const fipsRowLabel = computed(() => {
  const s = fipsSummary.value
  if (!s) return '…'
  if (!s.installed) return 'Not installed'
  if (!s.service_active) {
    if (!s.key_present) return 'Awaiting seed'
    return 'Inactive'
  }
  // Service is active — reflect anchor reachability so the row flips in
  // sync with the full FIPS card below.
  if (s.anchor_connected === false) return 'No anchor'
  const peers = s.authenticated_peer_count ?? 0
  return peers === 1 ? 'Active · 1 peer' : `Active · ${peers} peers`
})
const fipsRowTextClass = computed(() => {
  const s = fipsSummary.value
  if (!s || !s.installed) return 'text-white/40'
  if (!s.service_active) return 'text-white/60'
  if (s.anchor_connected === false) return 'text-orange-400'
  return 'text-green-400'
})
function loadFipsSummary() {
  return fipsSummaryRes.refresh()
}

function loadNetworkData() {
  return networkRes.refresh()
}

// VPN peer management
const showAddDeviceModal = ref(false)
const newPeerName = ref('')
const creatingPeer = ref(false)
const peerQrData = ref<{ qr_svg: string; config: string; peer_ip: string } | null>(null)
// Sanitize like TwoFactorSection's TOTP QR — the SVG is backend-generated,
// but v-html without a sanitizer is one compromised RPC away from XSS.
const sanitizedPeerQrSvg = computed(() =>
  DOMPurify.sanitize(peerQrData.value?.qr_svg ?? '', { USE_PROFILES: { svg: true } }),
)
const peerError = ref('')
const copiedConfig = ref(false)
// TTL 30s (near the default, matching Tor status/services) — persist:false:
// each peer carries an npub (Nostr identity), so this is VPN peer identity
// data per T-02-01, distinct from server.network-summary's own-node status.
const vpnPeersRes = useCachedResource<{ name: string; ip: string; type?: string; npub?: string }[]>({
  key: 'server.vpn-peers',
  immediate: false,
  ttlMs: 30_000,
  persist: false,
  fetcher: async (signal) => {
    const res = await rpcClient.call<{ peers: { name: string; ip: string }[] }>({ method: 'vpn.list-peers', signal, dedup: true, maxRetries: 1 })
    return res.peers || []
  },
})
const vpnPeers = computed(() => vpnPeersRes.data.value ?? [])

function loadVpnPeers() {
  return vpnPeersRes.refresh()
}

async function createPeer() {
  if (!newPeerName.value.trim()) return
  creatingPeer.value = true
  peerError.value = ''
  try {
    const res = await rpcClient.call<{ qr_svg: string; config: string; peer_ip: string }>({
      method: 'vpn.create-peer',
      params: { name: newPeerName.value.trim() },
    })
    peerQrData.value = res
    loadVpnPeers()
  } catch (e) {
    peerError.value = e instanceof Error ? e.message : 'Failed to create peer'
  } finally {
    creatingPeer.value = false
  }
}

const loadingPeerConfig = ref(false)
async function showPeerConfig(name: string) {
  showAddDeviceModal.value = true
  loadingPeerConfig.value = true
  peerError.value = ''
  try {
    const res = await rpcClient.call<{ qr_svg: string; config: string; peer_ip: string }>({
      method: 'vpn.peer-config',
      params: { name },
    })
    peerQrData.value = res
  } catch (e) {
    peerError.value = e instanceof Error ? e.message : 'Failed to load config'
  } finally {
    loadingPeerConfig.value = false
  }
}

const removingPeer = ref('')
async function removePeer(name: string) {
  removingPeer.value = name
  try {
    await rpcClient.call({ method: 'vpn.remove-peer', params: { name } })
    vpnPeersRes.optimistic(cur => (cur ?? []).filter(p => p.name !== name))
  } catch { /* ignore */ }
  finally { removingPeer.value = '' }
}

const showingNewDevice = ref(false)

function closeDeviceModal() {
  showAddDeviceModal.value = false
  peerQrData.value = null
  newPeerName.value = ''
  peerError.value = ''
  showingNewDevice.value = false
}

async function copyPeerConfig() {
  if (!peerQrData.value?.config) return
  try { await navigator.clipboard.writeText(peerQrData.value.config) } catch { /* fallback */ }
  copiedConfig.value = true
  setTimeout(() => { copiedConfig.value = false }, 2000)
}

// Network interfaces
interface NetworkInterface { name: string; type: string; state: string; mac: string; ipv4: string[] }
interface WifiNetwork { ssid: string; signal: number; security: string }

// TTL 10s (fast tier, alongside network-summary/disk-status) — persist:true:
// local interface names/MACs/IPs are this node's own hardware info, not
// peer or VPN identity.
const interfacesRes = useCachedResource<NetworkInterface[]>({
  key: 'server.interfaces',
  immediate: false,
  ttlMs: 10_000,
  persist: true,
  fetcher: async (signal) => {
    const res = await rpcClient.call<{ interfaces: NetworkInterface[] }>({ method: 'network.list-interfaces', signal, dedup: true, maxRetries: 1 })
    return res.interfaces
  },
})
const interfacesError = computed(() => interfacesRes.error.value)
const interfacesLoading = computed(() => interfacesRes.loadState.value === 'loading' || (interfacesRes.loadState.value === 'idle' && interfacesRes.data.value === null))
const interfacesRefreshing = computed(() => interfacesRes.loadState.value === 'refreshing')
const allInterfaces = computed(() => interfacesRes.data.value ?? [])
const physicalInterfaces = computed(() => allInterfaces.value.filter(i => i.type === 'ethernet' || i.type === 'wifi'))
const wifiAvailable = computed(() => allInterfaces.value.some(i => i.type === 'wifi'))

const togglingWifiRadio = ref(false)
const wifiRadioError = ref('')

const showWifiModal = ref(false)
const wifiScanning = ref(false)
const wifiNetworks = ref<WifiNetwork[]>([])
const wifiConnecting = ref(false)
const wifiSubmitting = ref(false)
const wifiSelectedSsid = ref('')
const wifiPassword = ref('')
const wifiError = ref('')
const wifiScanError = ref('')

// DNS
const showDnsModal = ref(false)
const dnsSelectedProvider = ref('system')
const dnsApplying = ref(false)
const dnsError = ref('')

const dnsProviderOptions = [
  { value: 'system', label: 'System Default', description: 'DHCP-assigned DNS servers', doh: false },
  { value: 'cloudflare', label: 'Cloudflare', description: '1.1.1.1 / 1.0.0.1', doh: true },
  { value: 'google', label: 'Google', description: '8.8.8.8 / 8.8.4.4', doh: true },
  { value: 'quad9', label: 'Quad9', description: '9.9.9.9 / 149.112.112.112', doh: true },
  { value: 'mullvad', label: 'Mullvad', description: '194.242.2.2 (no logging)', doh: true },
  { value: 'custom', label: 'Custom', description: 'Enter your own DNS servers', doh: false },
]

type DnsProviderValue = 'system' | 'cloudflare' | 'google' | 'quad9' | 'mullvad' | 'custom'

const dnsDisplayLabel = computed(() => {
  const p = networkData.value.dnsProvider
  const opt = dnsProviderOptions.find(o => o.value === p)
  if (opt && p !== 'system') return `${opt.label}${networkData.value.dnsDoH ? ' (DoH)' : ''}`
  if (networkData.value.dnsServers.length > 0) return networkData.value.dnsServers.slice(0, 2).join(', ')
  return 'System Default'
})

async function applyDnsConfig(customServers: string) {
  dnsApplying.value = true; dnsError.value = ''
  try {
    const provider = dnsSelectedProvider.value as DnsProviderValue
    const params: { provider: DnsProviderValue; servers?: string[] } = { provider }
    if (provider === 'custom') { params.servers = customServers.split(',').map(s => s.trim()).filter(s => s.length > 0) }
    const res = await rpcClient.configureDns(params)
    // Never trust the response shape: an undefined `servers` used to reach the
    // dnsDisplayLabel computed and crash the whole page render on `.length`.
    // Write-through to the cached aggregate (the RPC already succeeded).
    networkRes.optimistic(cur => ({
      ...(cur ?? defaultNetworkData()),
      dnsProvider: res?.provider ?? provider,
      dnsServers: Array.isArray(res?.servers) ? res.servers : (params.servers ?? []),
      dnsDoH: !!res?.doh_enabled,
    }))
    showDnsModal.value = false
  } catch (e) { dnsError.value = e instanceof Error ? e.message : 'DNS configuration failed.' } finally { dnsApplying.value = false }
}

function loadInterfaces() {
  return interfacesRes.refresh()
}

async function toggleWifiRadio(iface: NetworkInterface) {
  togglingWifiRadio.value = true
  wifiRadioError.value = ''
  const enabled = iface.state !== 'up'
  try {
    await rpcClient.call({ method: 'network.set-wifi-radio', params: { enabled } })
    await loadInterfaces()
  } catch (e) {
    wifiRadioError.value = e instanceof Error ? e.message : 'Failed to change wifi radio state.'
  } finally {
    togglingWifiRadio.value = false
  }
}

function wifiRequiresPassword(network: WifiNetwork | undefined): boolean {
  const security = (network?.security || '').trim().toLowerCase()
  return security.length > 0 && security !== '--' && security !== 'none' && security !== 'open'
}

async function scanWifi() {
  wifiScanning.value = true; wifiNetworks.value = []; wifiScanError.value = ''; wifiError.value = ''
  try {
    const res = await rpcClient.call<{ networks: WifiNetwork[] }>({ method: 'network.scan-wifi' })
    wifiNetworks.value = res.networks
  } catch (e) {
    wifiNetworks.value = []
    wifiScanError.value = e instanceof Error ? e.message : 'WiFi scan failed.'
  } finally { wifiScanning.value = false }
}

function selectWifi(network: WifiNetwork) {
  wifiSelectedSsid.value = network.ssid; wifiPassword.value = ''; wifiError.value = ''
  if (wifiRequiresPassword(network)) {
    wifiConnecting.value = true
  } else {
    connectToWifi('')
  }
}

async function connectToWifi(password: string) {
  if (!wifiSelectedSsid.value) return
  wifiError.value = ''; wifiSubmitting.value = true
  try {
    await rpcClient.call({ method: 'network.configure-wifi', params: { ssid: wifiSelectedSsid.value, password } })
    showWifiModal.value = false; wifiConnecting.value = false; wifiPassword.value = ''
    logsToast.value = 'WiFi connected successfully'; setTimeout(() => { logsToast.value = '' }, 4000); loadInterfaces()
  } catch (e) { wifiError.value = e instanceof Error ? e.message : 'WiFi connection failed.' } finally { wifiSubmitting.value = false }
}

// Disk space
const diskWarning = ref<{ level: 'warning' | 'critical'; used_percent: number; free_bytes: number } | null>(null)
const diskEncrypted = ref(false)
const diskCleaning = ref(false)

// 02-06: the seventh Server load group. TTL 10s (fast tier, same as
// network-summary/interfaces) — persist:true: disk usage figures carry no
// identity/financial payload. Was previously a plain uncached fetch called
// unconditionally on every activation (see armServerEntryEffects below,
// pre-02-06) — now behaves like the other six: this resource's own
// onActivated (useCachedResource.ts) revalidates it staleness-gated on
// reactivation, so the explicit every-entry call is no longer needed.
const diskStatusRes = useCachedResource<{ used_bytes: number; total_bytes: number; free_bytes: number; used_percent: number; level: 'ok' | 'warning' | 'critical'; encrypted?: boolean }>({
  key: 'server.disk-status',
  immediate: false,
  ttlMs: 10_000,
  persist: true,
  fetcher: () => rpcClient.diskStatus(),
})
function loadDiskStatus() {
  return diskStatusRes.refresh().then(() => {
    const res = diskStatusRes.data.value
    if (!res) return
    diskEncrypted.value = !!(res as Record<string, unknown>).encrypted
    if (res.level === 'warning' || res.level === 'critical') {
      diskWarning.value = { level: res.level, used_percent: res.used_percent, free_bytes: res.free_bytes }
    } else { diskWarning.value = null }
  })
}

async function runDiskCleanup() {
  diskCleaning.value = true
  try { await rpcClient.diskCleanup(); await loadDiskStatus(); logsToast.value = 'Disk cleanup completed'; setTimeout(() => { logsToast.value = '' }, 4000) }
  catch (e) { logsToast.value = `Disk cleanup failed: ${e instanceof Error ? e.message : 'Unknown error'}`; setTimeout(() => { logsToast.value = '' }, 6000) }
  finally { diskCleaning.value = false }
}

function formatBytes(bytes: number): string {
  const gb = 1024 * 1024 * 1024; const mb = 1024 * 1024
  if (bytes >= gb) return `${(bytes / gb).toFixed(1)} GB`
  if (bytes >= mb) return `${(bytes / mb).toFixed(0)} MB`
  return `${(bytes / 1024).toFixed(0)} KB`
}

// Tor Services
// TTL 30s (near the default) — persist:false: each service carries an
// onion_address, a Tor identity payload per T-02-01.
const torServicesRes = useCachedResource<{ services: TorServiceInfo[]; tor_running: boolean }>({
  key: 'server.tor-services',
  immediate: false,
  ttlMs: 30_000,
  persist: false,
  fetcher: async (signal) => {
    const res = await rpcClient.call<{ services: TorServiceInfo[]; tor_running: boolean }>({ method: 'tor.list-services', signal, dedup: true, maxRetries: 1 })
    return { services: res.services || [], tor_running: res.tor_running ?? false }
  },
})
const torServices = computed(() => torServicesRes.data.value?.services ?? [])
const torServicesLoading = computed(() =>
  torServicesRes.loadState.value === 'loading' || torServicesRes.loadState.value === 'refreshing')
const torDaemonRunning = computed(() => torServicesRes.data.value?.tor_running ?? false)
const torRestarting = ref(false)
const torRotating = ref<string | false>(false)
const torDeleting = ref<string | false>(false)

const showAddServiceModal = ref(false)
const addingService = ref(false)
const addServiceError = ref('')

const availableAppsForTor = computed(() => {
  const existingNames = new Set(torServices.value.map(s => s.name))
  return Object.entries(appStore.packages)
    .filter(([id]) => !existingNames.has(id))
    .map(([id, pkg]) => ({ id, title: (pkg as { manifest?: { title?: string } })?.manifest?.title || id }))
    .sort((a, b) => a.title.localeCompare(b.title))
})

function loadTorServices() {
  return torServicesRes.refresh()
}

// Drives the header RefreshIndicator (D-05): visible whenever any of the six
// underlying cache entries (torServicesRes also backs checkTorStatus) is
// refreshing in the background, so a stale-while-revalidate pass is never
// silent.
const serverRefreshIndicatorState = computed(() => {
  const states = [
    networkRes.loadState.value,
    fipsSummaryRes.loadState.value,
    vpnPeersRes.loadState.value,
    interfacesRes.loadState.value,
    torServicesRes.loadState.value,
    diskStatusRes.loadState.value,
  ]
  return states.includes('refreshing') ? 'refreshing' : 'ready'
})

async function copyTorAddress(address: string) {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(address)
    } else {
      const ta = document.createElement('textarea')
      ta.value = address
      ta.style.position = 'fixed'
      ta.style.opacity = '0'
      document.body.appendChild(ta)
      ta.select()
      document.execCommand('copy')
      document.body.removeChild(ta)
    }
    logsToast.value = 'Onion address copied to clipboard'
  } catch {
    logsToast.value = 'Failed to copy address'
  }
  setTimeout(() => { logsToast.value = '' }, 3000)
}

async function toggleTorApp(appId: string, enabled: boolean) { try { await rpcClient.call({ method: 'tor.toggle-app', params: { app_id: appId, enabled }, timeout: 90000 }); await loadTorServices() } catch { /* handled */ } }
async function rotateService(name: string) { torRotating.value = name; try { await rpcClient.call({ method: 'tor.rotate-service', params: { name }, timeout: 90000 }); await loadTorServices() } catch { /* handled */ } finally { torRotating.value = false } }
async function restartTor() { torRestarting.value = true; try { await rpcClient.call({ method: 'tor.restart', timeout: 90000 }); await loadTorServices(); logsToast.value = 'Tor restarted successfully'; setTimeout(() => { logsToast.value = '' }, 3000) } catch { logsToast.value = 'Failed to restart Tor'; setTimeout(() => { logsToast.value = '' }, 5000) } finally { torRestarting.value = false } }
async function deleteService(name: string) { torDeleting.value = name; try { await rpcClient.call({ method: 'tor.delete-service', params: { name }, timeout: 90000 }); await loadTorServices(); logsToast.value = `Tor service "${name}" deleted`; setTimeout(() => { logsToast.value = '' }, 3000) } catch { /* handled */ } finally { torDeleting.value = false } }

async function createServiceForApp(appId: string) {
  addServiceError.value = ''; addingService.value = true
  try { await rpcClient.call({ method: 'tor.create-service', params: { name: appId, local_port: 0 }, timeout: 90000 }); showAddServiceModal.value = false; await loadTorServices(); logsToast.value = `Tor service for "${appId}" created`; setTimeout(() => { logsToast.value = '' }, 3000) }
  catch (e) { addServiceError.value = e instanceof Error ? e.message : 'Failed to create service' } finally { addingService.value = false }
}

async function createService(name: string, port: number | null) {
  if (!name || !port) return
  addServiceError.value = ''; addingService.value = true
  try { await rpcClient.call({ method: 'tor.create-service', params: { name, local_port: port }, timeout: 90000 }); showAddServiceModal.value = false; await loadTorServices(); logsToast.value = `Tor service "${name}" created`; setTimeout(() => { logsToast.value = '' }, 3000) }
  catch (e) { addServiceError.value = e instanceof Error ? e.message : 'Failed to create service' } finally { addingService.value = false }
}

// Only-while-visible: polls VPN status every 15s so IP updates after pairing
// (write-through to the cached aggregate without refetching the other three
// RPCs). An off-screen Server tab must not keep polling (T-02-03) — armed on
// activation, cleared on deactivation; idempotent (clears any existing
// handle first, so two consecutive activations never double-arm).
let vpnPollInterval: ReturnType<typeof setInterval> | null = null

async function pollVpnStatusOnce() {
  try {
    const vpnRes = await rpcClient.vpnStatus()
    networkRes.optimistic(cur => ({
      ...(cur ?? defaultNetworkData()),
      vpnConnected: vpnRes.connected,
      vpnProvider: vpnRes.provider ?? '',
      vpnIp: (vpnRes.ip_address ?? '').replace(/\/\d+$/, ''),
      wgIp: vpnRes.wg_ip ?? '',
    }))
  } catch { /* ignore */ }
}
function armVpnPoll() {
  if (vpnPollInterval) clearInterval(vpnPollInterval)
  // Immediate first tick on (re)activation — a re-entered tab must not sit
  // on a 15s-stale VPN IP waiting out the interval.
  void pollVpnStatusOnce()
  vpnPollInterval = setInterval(() => void pollVpnStatusOnce(), 15000)
}
function disarmVpnPoll() {
  if (vpnPollInterval) { clearInterval(vpnPollInterval); vpnPollInterval = null }
}

// 02-06: loadDiskStatus is now the seventh useCachedResource-backed load
// (see diskStatusRes above), so it self-heals on reactivation exactly like
// the other six — armServerEntryEffects only needs to re-arm the VPN poll
// interval now, not force-call loadDiskStatus on every entry.
function armServerEntryEffects() {
  armVpnPoll()
}

// Vue fires onActivated immediately after onMounted on a KeepAlive-wrapped
// component's first mount — this flag lets onMounted's call count as the
// first activation's arm, so onActivated only re-arms (and re-issues the
// immediate VPN poll tick) on a genuine later reactivation, not redundantly
// right after mount.
let serverFreshMount = true
onActivated(() => {
  if (serverFreshMount) { serverFreshMount = false; return }
  armServerEntryEffects()
})
onDeactivated(() => disarmVpnPoll())
onUnmounted(() => disarmVpnPoll())

// Once-per-session seed for all seven useCachedResource-backed loads below
// (network-summary, fips-summary, vpn-peers, interfaces, tor-services,
// disk-status, and checkTorStatus which shares tor-services): each
// resource's own onActivated (added in useCachedResource.ts) already
// revalidates it, staleness-gated, on every later reactivation — same
// pattern established for Marketplace.vue in 02-02. 02-06 settled RESEARCH
// A3 (see 02-06-SUMMARY.md) — none of these seven loaders consumes another's
// result, so this fan-out stays fire-and-forget/concurrent, never chained.
//
// Also arms the every-entry effects directly here: onActivated is a no-op
// outside a <KeepAlive> boundary (confirmed by ServerNetworkRefresh.test.ts,
// which mounts this view bare), so a bare mount must not silently skip them.
onMounted(() => {
  consumeOpenWifiQuery()
  checkTorStatus(); loadNetworkData(); loadInterfaces(); loadTorServices(); loadVpnPeers(); loadFipsSummary(); loadDiskStatus()
  armServerEntryEffects()
})

watch(showWifiModal, (open) => { if (open) scanWifi() })
watch(showDnsModal, (open) => { if (open) { dnsSelectedProvider.value = networkData.value.dnsProvider || 'system'; dnsError.value = '' } })

// #145: onboarding's no-network callout deep-links here with ?open=wifi so a
// fresh-install user lands straight in the WiFi picker. Read from the real
// URL (the dashboard's SPA router keeps it in sync) rather than vue-router —
// the KeepAlive-mounted view has no router guarantee at test-mount time —
// and consume it (history.replaceState) so a tab-return never re-pops.
function consumeOpenWifiQuery() {
  const params = new URLSearchParams(window.location.search)
  if (params.get('open') !== 'wifi') return
  params.delete('open')
  const qs = params.toString()
  history.replaceState(history.state, '', window.location.pathname + (qs ? `?${qs}` : '') + window.location.hash)
  showWifiModal.value = true
}

async function restartServices() {
  restarting.value = true; servicesRunning.value = false
  try { await rpcClient.restartServer(); logsToast.value = 'Services restarting...'; setTimeout(() => { logsToast.value = '' }, 4000) }
  catch (e) { logsToast.value = `Restart failed: ${e instanceof Error ? e.message : 'Unknown error'}`; setTimeout(() => { logsToast.value = '' }, 6000) }
  const pollHealth = async (retries: number) => {
    for (let i = 0; i < retries; i++) {
      await new Promise(r => setTimeout(r, 2000))
      try { await rpcClient.call({ method: 'server.health', params: {} }); servicesRunning.value = true; restarting.value = false; return } catch { /* still restarting */ }
    }
    restarting.value = false; servicesRunning.value = false; torStatusLabel.value = 'stopped'
  }
  pollHealth(15)
}

async function checkTorStatus() {
  checkingTor.value = true; torStatusLabel.value = 'checking'
  try {
    await torServicesRes.refresh()
    // Report the DAEMON, not whether onion addresses exist. A .onion address is
    // a config artifact — the hidden-service hostname file stays on disk when
    // Tor is dead — so `some(s => s.onion_address)` reported "running" on three
    // fleet nodes whose Tor had been down for days (2026-08-09). The backend
    // already returns the truth: tor_running comes from an actual connect to
    // 127.0.0.1:9050.
    if (torServicesRes.error.value) torStatusLabel.value = 'stopped'
    else torStatusLabel.value = torDaemonRunning.value ? 'running' : 'stopped'
  } finally { checkingTor.value = false }
}

const logsToast = ref('')
function viewLogs() { logCount.value = 0; logsToast.value = 'Server logs are available via SSH: journalctl -u archipelago -f'; setTimeout(() => { logsToast.value = '' }, 6000) }

defineExpose({
  allInterfaces,
  interfacesRefreshing,
  loadInterfaces,
  loadNetworkData,
  loadTorServices,
  loadDiskStatus,
  loadVpnPeers,
  loadFipsSummary,
  networkData,
  networkRefreshing,
  torServices,
  torServicesLoading,
  serverRefreshIndicatorState,
})
</script>
