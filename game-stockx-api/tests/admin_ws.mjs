import assert from 'node:assert/strict';
let input = '';
for await (const chunk of process.stdin) input += chunk;
const { base, admin, victim, victimId, observer } = JSON.parse(input);
const connections = [];
async function connect(token) {
  const socket = new WebSocket(base.replace(/^http/, 'ws') + '/ws/');
  connections.push(socket);
  const messages = [];
  socket.addEventListener('message', e => messages.push(JSON.parse(e.data)));
  const closed = new Promise(resolve => socket.addEventListener('close', resolve));
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  socket.send(JSON.stringify({ type: 'authenticate', token }));
  return { socket, closed, messages };
}
async function until(predicate) {
  for (let i = 0; i < 100; i++) {
    if (predicate()) return;
    await new Promise(resolve => setTimeout(resolve, 25));
  }
  throw new Error('WebSocket condition timed out');
}
try {
  const watcher = await connect(observer);
  const a = await connect(victim);
  const b = await connect(victim);
  await until(() => a.messages.some(m => m.type === 'authenticated') && b.messages.some(m => m.type === 'authenticated'));
  await until(() => watcher.messages.some(m => m.online?.includes('victim')));
  const response = await fetch(`${base}/api/admin/users/${victimId}`, { method: 'DELETE', headers: { Authorization: `Bearer ${admin}` } });
  assert.equal(response.status, 204);
  for (const session of [a,b]) {
    const event = await Promise.race([session.closed, new Promise((_,reject) => setTimeout(() => reject(new Error('Session was not revoked')), 3000))]);
    assert.equal(event.code, 1008);
  }
  await until(() => watcher.messages.at(-1)?.online?.every(login => login !== 'victim'));
  const retry = await connect(victim);
  assert.equal((await retry.closed).code, 1008);
  console.log('PASS: deleting an account disconnects all its chat tabs, updates presence and rejects reconnection');
} finally { connections.forEach(socket => socket.close()); }
