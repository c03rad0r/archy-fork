#!/bin/bash

echo "🔍 Testing ATOB Installation Setup"
echo "===================================="

# Check if s9pk exists
echo ""
echo "1. Checking s9pk file..."
if [ -f "public/packages/atob.s9pk" ]; then
    echo "   ✅ Found: public/packages/atob.s9pk ($(du -h public/packages/atob.s9pk | cut -f1))"
else
    echo "   ❌ Missing: public/packages/atob.s9pk"
    exit 1
fi

# Check if mock backend is running
echo ""
echo "2. Checking mock backend..."
if lsof -i :5959 | grep LISTEN > /dev/null; then
    echo "   ✅ Mock backend running on port 5959"
else
    echo "   ❌ Mock backend NOT running on port 5959"
    echo "   Run: node mock-backend.js"
    exit 1
fi

# Check if Docker is running
echo ""
echo "3. Checking Docker..."
if docker ps > /dev/null 2>&1; then
    echo "   ✅ Docker is running"
else
    echo "   ❌ Docker is NOT running"
    echo "   Start Docker Desktop"
    exit 1
fi

# Test RPC call
echo ""
echo "4. Testing RPC endpoint..."
RESPONSE=$(curl -s -X POST http://localhost:5959/rpc/v1 \
  -H "Content-Type: application/json" \
  -d '{"method":"server.echo","params":{"message":"test"}}')

if echo "$RESPONSE" | grep -q '"result"'; then
    echo "   ✅ RPC endpoint responding"
else
    echo "   ❌ RPC endpoint not responding"
    echo "   Response: $RESPONSE"
    exit 1
fi

# Check existing containers
echo ""
echo "5. Checking existing atob containers..."
if docker ps -a | grep atob-test > /dev/null; then
    echo "   ⚠️  Found existing atob-test container"
    echo "   To remove: docker rm -f atob-test"
else
    echo "   ✅ No existing atob-test container"
fi

echo ""
echo "===================================="
echo "✅ All checks passed!"
echo ""
echo "Now test in browser:"
echo "1. Go to http://localhost:8100"
echo "2. Navigate to Marketplace"
echo "3. Click Install on ATOB"
echo "4. Watch terminal for Docker logs"
echo "5. Watch browser console for polling logs"

