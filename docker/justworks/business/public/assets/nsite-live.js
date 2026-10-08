const prefix = location.pathname.split('/').slice(0, -1).join('/');
const mount = document.querySelector('#mount');
let syncing = false;
addEventListener('message',event=>{const frame=mount.querySelector('iframe');if(!frame||event.source!==frame.contentWindow||event.data?.type!=='napplet.resize')return;const height=Math.max(320,Math.min(4000,Number(event.data.height)||0));frame.style.height=`${height}px`});

async function tableToken() {
  const existing = new URLSearchParams(location.search).get('table');
  if (existing) return existing;
  const dashboard = await fetch(`${prefix}/api/dashboard`, { cache: 'no-store' }).then(response => response.json());
  const table = dashboard.tables.find(item => item.table_number === 21) || dashboard.tables[0];
  history.replaceState(null, '', `${location.pathname}?table=${encodeURIComponent(table.token)}`);
  return table.token;
}

async function syncActivation() {
  if (syncing) return;
  syncing = true;
  try {
    const config = await fetch(`${prefix}/api/config`, { cache: 'no-store' }).then(response => response.json());
    const frame = mount.querySelector('iframe');
    if (!config.module_enabled && frame) {
      mount.className = 'napplet off';
      mount.textContent = 'Order to Table is currently disabled by the merchant.';
    } else if (config.module_enabled && !frame) {
      const token = await tableToken();
      mount.className = 'napplet';
      mount.innerHTML = `<iframe title="Order to Table napplet shell" src="${prefix}/embed?table=${encodeURIComponent(token)}"></iframe>`;
    }
  } finally {
    syncing = false;
  }
}

window.setInterval(() => syncActivation().catch(() => {}), 700);
