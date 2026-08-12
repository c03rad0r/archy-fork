import express from 'express';

const app = express();
const port = 8080;

// Middleware
app.use(express.json());
app.use(express.static('public'));

// Health check endpoint
app.get('/health', (req, res) => {
  res.json({ status: 'ok', service: 'did-wallet' });
});

// Wallet API endpoints
app.get('/api/wallet/info', (req, res) => {
  res.json({
    status: 'ok',
    wallet: {
      dids: [],
      balance: 0
    }
  });
});

app.post('/api/wallet/did/create', async (req, res) => {
  // Placeholder for DID creation
  res.json({
    status: 'ok',
    did: 'did:key:placeholder'
  });
});

// Start server
app.listen(port, '0.0.0.0', () => {
  console.log(`DID Wallet listening on port ${port}`);
});
