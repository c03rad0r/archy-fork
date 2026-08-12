// Dummy apps data for first launch
// This can be easily replaced with real package data later
// Similar to how atob and k484 are handled

import type { PackageDataEntry } from '../types/api'
import { PackageState, ServiceStatus } from '../types/api'

export const dummyApps: Record<string, PackageDataEntry> = {
  'bitcoin': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'Bitcoin Core node for the Neode network',
      icon: '/assets/img/app-icons/bitcoin.svg'
    },
    manifest: {
      id: 'bitcoin',
      title: 'Bitcoin Core',
      version: '24.0.0',
      description: {
        short: 'Full Bitcoin node implementation',
        long: 'Bitcoin Core is the reference implementation of Bitcoin. It provides a full node implementation that validates and relays transactions, maintains the blockchain, and serves as a wallet.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/bitcoin/bitcoin',
      'upstream-repo': 'https://github.com/bitcoin/bitcoin',
      'support-site': 'https://github.com/bitcoin/bitcoin/issues',
      'marketing-site': 'https://bitcoin.org',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'bitcoin.onion',
          'lan-address': 'http://localhost:18443'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'btcpay-server': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'BTCPay Server payment processor',
      icon: '/assets/img/app-icons/btcpay-server.png'
    },
    manifest: {
      id: 'btcpay-server',
      title: 'BTCPay Server',
      version: '1.12.0',
      description: {
        short: 'Self-hosted Bitcoin payment processor',
        long: 'BTCPay Server is a free, open-source cryptocurrency payment processor. Accept Bitcoin payments without intermediaries or fees. Complete merchant solution with invoicing, point of sale, and more.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/btcpayserver/btcpayserver',
      'upstream-repo': 'https://github.com/btcpayserver/btcpayserver',
      'support-site': 'https://github.com/btcpayserver/btcpayserver/issues',
      'marketing-site': 'https://btcpayserver.org',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'btcpay.onion',
          'lan-address': 'http://localhost:14142'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'homeassistant': {
    state: PackageState.Running,
    'static-files': {
      license: 'Apache-2.0',
      instructions: 'Home automation platform',
      icon: '/assets/img/app-icons/homeassistant.png'
    },
    manifest: {
      id: 'homeassistant',
      title: 'Home Assistant',
      version: '2024.1.0',
      description: {
        short: 'Open source home automation platform',
        long: 'Home Assistant is an open-source home automation platform running on Python. It tracks and controls all devices at home and offers a platform for automating control.'
      },
      'release-notes': 'Initial release',
      license: 'Apache-2.0',
      'wrapper-repo': 'https://github.com/home-assistant/core',
      'upstream-repo': 'https://github.com/home-assistant/core',
      'support-site': 'https://github.com/home-assistant/core/issues',
      'marketing-site': 'https://www.home-assistant.io',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'homeassistant.onion',
          'lan-address': 'http://localhost:8123'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'lorabell': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'A LoRa based doorbell',
      icon: '/assets/img/app-icons/lorabell.png'
    },
    manifest: {
      id: 'lorabell',
      title: 'LoraBell',
      version: '1.0.0',
      description: {
        short: 'A LoRa based doorbell',
        long: 'A LoRa based doorbell - receive doorbell notifications over LoRa radio.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': '#',
      'upstream-repo': '#',
      'support-site': '#',
      'marketing-site': '#',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'lorabell.onion',
          'lan-address': 'http://192.0.2.13'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'grafana': {
    state: PackageState.Running,
    'static-files': {
      license: 'Apache-2.0',
      instructions: 'Analytics and monitoring platform',
      icon: '/assets/img/grafana.png'
    },
    manifest: {
      id: 'grafana',
      title: 'Grafana',
      version: '10.2.0',
      description: {
        short: 'Analytics and monitoring platform',
        long: 'Grafana is an open-source analytics and monitoring platform. Create dashboards, query metrics, and visualize data from multiple sources.'
      },
      'release-notes': 'Initial release',
      license: 'Apache-2.0',
      'wrapper-repo': 'https://github.com/grafana/grafana',
      'upstream-repo': 'https://github.com/grafana/grafana',
      'support-site': 'https://github.com/grafana/grafana/issues',
      'marketing-site': 'https://grafana.com',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'grafana.onion',
          'lan-address': 'http://localhost:3000'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'endurain': {
    state: PackageState.Stopped,
    'static-files': {
      license: 'MIT',
      instructions: 'Endurain application',
      icon: '/assets/img/endurain.png'
    },
    manifest: {
      id: 'endurain',
      title: 'Endurain',
      version: '1.0.0',
      description: {
        short: 'Endurain application platform',
        long: 'Endurain provides a platform for decentralized applications and services.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/endurain/endurain',
      'upstream-repo': 'https://github.com/endurain/endurain',
      'support-site': 'https://github.com/endurain/endurain/issues',
      'marketing-site': 'https://endurain.io',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'endurain.onion',
          'lan-address': 'http://localhost:8084'
        }
      },
      status: ServiceStatus.Stopped
    }
  },
  'fedimint': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'Federated Bitcoin mint',
      icon: '/assets/img/app-icons/fedimint.png'
    },
    manifest: {
      id: 'fedimint',
      title: 'Fedimint',
      version: '0.3.0',
      description: {
        short: 'Federated Bitcoin minting service',
        long: 'Fedimint is a federated Bitcoin mint that enables private, scalable Bitcoin transactions through a federation of guardians.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/fedimint/fedimint',
      'upstream-repo': 'https://github.com/fedimint/fedimint',
      'support-site': 'https://github.com/fedimint/fedimint/issues',
      'marketing-site': 'https://fedimint.org',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'fedimint.onion',
          'lan-address': 'http://localhost:8175'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'morphos-server': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'MorphOS server application',
      icon: '/assets/img/morphos.png'
    },
    manifest: {
      id: 'morphos-server',
      title: 'MorphOS Server',
      version: '1.0.0',
      description: {
        short: 'MorphOS server platform',
        long: 'MorphOS Server provides a flexible server platform for various applications and services.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/morphos/morphos',
      'upstream-repo': 'https://github.com/morphos/morphos',
      'support-site': 'https://github.com/morphos/morphos/issues',
      'marketing-site': 'https://morphos.io',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'morphos.onion',
          'lan-address': 'http://localhost:8081'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'lightning-stack': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'Lightning Network stack',
      icon: '/assets/img/app-icons/lightning-stack.png'
    },
    manifest: {
      id: 'lightning-stack',
      title: 'Lightning Stack',
      version: '0.12.0',
      description: {
        short: 'Complete Lightning Network implementation',
        long: 'Lightning Stack provides a complete Lightning Network node implementation with LND, CLN, and supporting services for fast Bitcoin transactions.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/lightningnetwork/lnd',
      'upstream-repo': 'https://github.com/lightningnetwork/lnd',
      'support-site': 'https://github.com/lightningnetwork/lnd/issues',
      'marketing-site': 'https://lightning.network',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'lightning.onion',
          'lan-address': 'http://localhost:8080'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'mempool': {
    state: PackageState.Running,
    'static-files': {
      license: 'AGPL-3.0',
      instructions: 'Bitcoin mempool explorer',
      icon: '/assets/img/app-icons/mempool.png'
    },
    manifest: {
      id: 'mempool',
      title: 'Mempool',
      version: '2.5.0',
      description: {
        short: 'Bitcoin mempool and blockchain explorer',
        long: 'Mempool is an open-source Bitcoin mempool and blockchain explorer. View transactions, blocks, and network statistics in real-time.'
      },
      'release-notes': 'Initial release',
      license: 'AGPL-3.0',
      'wrapper-repo': 'https://github.com/mempool/mempool',
      'upstream-repo': 'https://github.com/mempool/mempool',
      'support-site': 'https://github.com/mempool/mempool/issues',
      'marketing-site': 'https://mempool.space',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'mempool.onion',
          'lan-address': 'http://localhost:4080'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'ollama': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'Local AI model runner',
      icon: '/assets/img/app-icons/ollama.png'
    },
    manifest: {
      id: 'ollama',
      title: 'Ollama',
      version: '0.1.0',
      description: {
        short: 'Run large language models locally',
        long: 'Ollama allows you to run large language models locally on your Neode server. Download and run models like Llama, Mistral, and more without cloud dependencies.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/ollama/ollama',
      'upstream-repo': 'https://github.com/ollama/ollama',
      'support-site': 'https://github.com/ollama/ollama/issues',
      'marketing-site': 'https://ollama.ai',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'ollama.onion',
          'lan-address': 'http://localhost:11434'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'searxng': {
    state: PackageState.Running,
    'static-files': {
      license: 'AGPL-3.0',
      instructions: 'Privacy-respecting search engine',
      icon: '/assets/img/app-icons/searxng.png'
    },
    manifest: {
      id: 'searxng',
      title: 'SearXNG',
      version: '2024.1.0',
      description: {
        short: 'Privacy-respecting metasearch engine',
        long: 'SearXNG is a privacy-respecting, hackable metasearch engine. Aggregate results from multiple search engines without tracking or ads.'
      },
      'release-notes': 'Initial release',
      license: 'AGPL-3.0',
      'wrapper-repo': 'https://github.com/searxng/searxng',
      'upstream-repo': 'https://github.com/searxng/searxng',
      'support-site': 'https://github.com/searxng/searxng/issues',
      'marketing-site': 'https://searxng.org',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': 'searxng.onion',
          'lan-address': 'http://localhost:8082'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'indeedhub': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'Decentralized media streaming platform',
      icon: '/assets/img/app-icons/indeedhub.png'
    },
    manifest: {
      id: 'indeedhub',
      title: 'Indeehub',
      version: '0.1.0',
      description: {
        short: 'Decentralized media streaming platform',
        long: 'Indeehub is a decentralized media streaming platform built on Nostr. Stream Bitcoin-focused documentaries, educational content, and independent films. Netflix-inspired interface with glassmorphism design, supporting content creators through the decentralized web.'
      },
      'release-notes': 'Initial release with Netflix-inspired interface',
      license: 'MIT',
      'wrapper-repo': 'https://github.com/indeedhub/indeedhub',
      'upstream-repo': 'https://github.com/indeedhub/indeedhub',
      'support-site': 'https://github.com/indeedhub/indeedhub/issues',
      'marketing-site': 'https://indeedhub.com',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: {
          'tor-address': '',
          'lan-address': 'http://localhost:7778'
        }
      },
      status: ServiceStatus.Running
    }
  },
  'botfights': {
    state: PackageState.Running,
    'static-files': {
      license: 'MIT',
      instructions: 'AI bot arena',
      icon: '/assets/img/app-icons/botfights.svg'
    },
    manifest: {
      id: 'botfights',
      title: 'BotFights',
      version: '1.0.0',
      description: {
        short: 'AI bot arena — build, train, and battle autonomous agents',
        long: 'BotFights is an AI bot arena where you can build, train, and battle autonomous agents. Create intelligent bots using various strategies, pit them against other players\' creations, and climb the leaderboard. Features real-time battle visualization, multiple game modes, and a growing community of bot builders.'
      },
      'release-notes': 'Initial release',
      license: 'MIT',
      'wrapper-repo': '',
      'upstream-repo': '',
      'support-site': '',
      'marketing-site': 'https://botfights.net',
      website: 'https://botfights.net',
      'donation-url': null
    },
    installed: {
      'current-dependents': {},
      'current-dependencies': {},
      'last-backup': null,
      'interface-addresses': {
        main: { 'tor-address': '', 'lan-address': 'http://localhost:9100' }
      },
      status: ServiceStatus.Running
    }
  },
}
