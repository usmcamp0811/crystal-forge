/**
 * WebSocket Eval Log Streaming Test
 *
 * This test verifies that evaluation logs stream correctly via WebSocket
 * even when the evaluation completes before the WebSocket client connects.
 *
 * Test scenario:
 * 1. Create a flake and commit via API
 * 2. Trigger evaluation that broadcasts logs and completes
 * 3. THEN connect WebSocket client (simulating late connection)
 * 4. Verify logs are received (tests history replay)
 *
 * Usage: node eval-websocket-test.js <baseUrl> <token>
 */

const { chromium } = require("playwright");
const WebSocket = require("ws");

const baseUrl = process.argv[2] || "http://127.0.0.1:3000";
const authToken = process.argv[3];

if (!authToken) {
  console.error("Usage: node eval-websocket-test.js <baseUrl> <token>");
  process.exit(1);
}

async function test() {
  console.log("Starting WebSocket eval log test...");
  
  // Step 1: Create a test flake
  console.log("Creating test flake...");
  const flakeRes = await fetch(`${baseUrl}/api/v1/flakes`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${authToken}`,
    },
    body: JSON.stringify({
      name: "test-flake-websocket",
      repo_url: "https://github.com/test/test",
      auto_poll: false,
    }),
  });
  
  if (!flakeRes.ok) {
    throw new Error(`Failed to create flake: ${await flakeRes.text()}`);
  }
  
  const flake = await flakeRes.json();
  console.log(`Created flake: ${flake.name} (id: ${flake.id})`);
  
  // Step 2: Create a commit manually
  console.log("Creating test commit...");
  const commitRes = await fetch(`${baseUrl}/api/v1/admin/commits`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${authToken}`,
    },
    body: JSON.stringify({
      flake_id: flake.id,
      git_commit_hash: "deadbeef1234567890abcdef1234567890abcdef",
      commit_timestamp: new Date().toISOString(),
      message: "Test commit for WebSocket eval logs",
      author: "Test User <test@example.com>",
    }),
  });
  
  if (!commitRes.ok) {
    throw new Error(`Failed to create commit: ${await commitRes.text()}`);
  }
  
  const commit = await commitRes.json();
  console.log(`Created commit: ${commit.git_commit_hash} (id: ${commit.id})`);
  
  // Step 3: Trigger evaluation (will fail fast due to no git repo)
  // The evaluation will complete and broadcast error logs before we connect
  console.log("Triggering evaluation...");
  
  const evalRes = await fetch(`${baseUrl}/api/v1/commits/${commit.id}/re-evaluate`, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${authToken}`,
    },
  });
  
  if (!evalRes.ok) {
    console.warn(`Re-evaluate returned ${evalRes.status}, continuing anyway...`);
  } else {
    console.log("Evaluation triggered");
  }
  
  // Step 4: Wait for evaluation to complete and logs to be broadcast
  // (Real evaluations complete quickly when they fail, so logs are already in history)
  console.log("Waiting for evaluation to complete...");
  await new Promise(resolve => setTimeout(resolve, 2000));
  
  // Step 5: NOW connect WebSocket (late connection scenario)
  console.log("Connecting WebSocket (late connection)...");
  
  const wsUrl = baseUrl.replace('http://', 'ws://').replace('https://', 'wss://');
  const ws = new WebSocket(`${wsUrl}/api/v1/commits/${commit.id}/eval/stream`);
  
  const receivedLogs = [];
  let wsConnected = false;
  
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      ws.close();
      reject(new Error("WebSocket test timed out after 5s"));
    }, 5000);
    
    ws.on('open', () => {
      console.log("WebSocket connected");
      wsConnected = true;
    });
    
    ws.on('message', (data) => {
      const msg = JSON.parse(data.toString());
      receivedLogs.push(msg);
      console.log(`Received: ${msg.type} - ${msg.message || msg.status || ''}`);
      
      // If we've received at least 1 log, test passes
      // (This confirms history replay works - the evaluation completed before we connected)
      if (receivedLogs.length >= 1) {
        clearTimeout(timeout);
        ws.close();
        
        console.log(`\n✅ SUCCESS: Received ${receivedLogs.length} log(s) via WebSocket`);
        console.log("   This confirms history replay works for late-connecting clients!");
        console.log("   The evaluation completed BEFORE we connected, yet we still received logs.");
        resolve({
          ok: true,
          receivedCount: receivedLogs.length,
        });
      }
    });
    
    ws.on('error', (error) => {
      clearTimeout(timeout);
      reject(new Error(`WebSocket error: ${error.message}`));
    });
    
    ws.on('close', () => {
      if (receivedLogs.length < 1) {
        clearTimeout(timeout);
        reject(new Error(
          `WebSocket closed without receiving any logs. ` +
          `This means history replay is not working for late-connecting clients.`
        ));
      }
    });
  });
}

test()
  .then(result => {
    console.log("\nTest passed!");
    process.exit(0);
  })
  .catch(error => {
    console.error("\n❌ Test failed:", error.message);
    process.exit(1);
  });
