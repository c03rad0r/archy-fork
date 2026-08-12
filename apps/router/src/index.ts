import express from 'express';
import bonjour from 'bonjour';

const app = express();
const port = 8080;

// Initialize Bonjour for mDNS
const bonjourInstance = bonjour();

// Publish Archipelago Router service
bonjourInstance.publish({
  name: 'Archipelago Router',
  type: 'http',
  port: port,
  txt: {
    version: '1.0.0',
    mesh: 'enabled',
    discovery: 'enabled'
  }
});

// Middleware
app.use(express.json());

// Health check endpoint
app.get('/health', (req, res) => {
  res.json({ status: 'ok', service: 'archipelago-router' });
});

// Network topology endpoint
app.get('/api/topology', (req, res) => {
  res.json({
    nodes: [],
    links: [],
    timestamp: Date.now()
  });
});

// Device discovery endpoint
app.get('/api/devices', (req, res) => {
  res.json({
    devices: [],
    count: 0
  });
});

// Start server
app.listen(port, '0.0.0.0', () => {
  console.log(`Archipelago Router listening on port ${port}`);
  console.log('mDNS service published');
});

// Graceful shutdown
process.on('SIGTERM', () => {
  console.log('Shutting down...');
  bonjourInstance.unpublishAll(() => {
    process.exit(0);
  });
});
