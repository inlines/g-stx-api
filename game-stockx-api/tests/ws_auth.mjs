// Node 22+; credentials are supplied through stdin, never through URL or argv.
import assert from 'node:assert/strict';
let input = '';
for await (const chunk of process.stdin) input += chunk;
const { base, alice, bob } = JSON.parse(input);
const connections = [];
function connect(path) {
  const socket = new WebSocket(base + path);
  connections.push(socket);
  const messages = [];
  const waiters = [];
  socket.addEventListener('message', (event) => {
    const value = JSON.parse(event.data);
    if (waiters.length) waiters.shift()(value); else messages.push(value);
  });
  const closed = new Promise((resolve) => socket.addEventListener('close', resolve));
  const ready = new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  const next = () => Promise.race([
    messages.length ? Promise.resolve(messages.shift()) : new Promise((resolve) => waiters.push(resolve)),
    new Promise((_, reject) => { const timer = setTimeout(() => reject(new Error('WS response timeout')), 3000); timer.unref(); }),
  ]);
  return { socket, messages, ready, closed, next, send: (data) => socket.send(JSON.stringify(data)) };
}
try {
  for (const frame of [{ sender: 'alice', recipient: 'bob', body: 'blocked' }, { type: 'authenticate', token: 'invalid' }]) {
    const client = connect('/ws/alice'); await client.ready; client.send(frame);
    assert.equal((await client.closed).code, 1008);
    assert.equal(client.messages.length, 0);
  }
  const idle = connect('/ws/alice'); await idle.ready;
  assert.equal((await idle.closed).code, 1008);
  assert.equal(idle.messages.length, 0);

  const a = connect('/ws/bob'); await a.ready; a.send({ type: 'authenticate', token: alice });
  assert.deepEqual(await a.next(), { type: 'authenticated', login: 'alice' });
  assert.deepEqual(await a.next(), { type: 'presence', online: ['alice'] });
  const b = connect('/ws/'); await b.ready; b.send({ type: 'authenticate', token: bob });
  assert.deepEqual(await b.next(), { type: 'authenticated', login: 'bob' });
  assert.deepEqual(await b.next(), { type: 'presence', online: ['alice', 'bob'] });
  a.send({ sender: 'bob', recipient: 'bob', body: 'token-bound sender', created_at: 'fake' });
  const message = await b.next();
  assert.equal(message.sender, 'alice');
  assert.equal(message.recipient, 'bob');
  assert.equal(message.body, 'token-bound sender');
  assert.notEqual(message.created_at, 'fake');
  console.log('PASS: missing/invalid authentication, timeout, spoofed URL and sender, authenticated presence and delivery');
} finally { connections.forEach((socket) => socket.close()); }
