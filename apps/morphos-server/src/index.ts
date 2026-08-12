import express from 'express';

const app = express();
const port = 8080;

// Middleware
app.use(express.json());

// Health check endpoint
app.get('/health', (req, res) => {
  res.json({ status: 'ok', service: 'morphos-server', version: '1.0.0' });
});

// API endpoints
app.get('/api/info', (req, res) => {
  res.json({
    name: 'MorphOS Server',
    version: '1.0.0',
    status: 'running'
  });
});

// Start server
app.listen(port, '0.0.0.0', () => {
  console.log(`MorphOS Server listening on port ${port}`);
  console.log(`Data directory: ${process.env.MORPHOS_DATA_DIR || '/app/data'}`);
});
