import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
let input = '';
for await (const chunk of process.stdin) input += chunk;
const { base, admin, ordinary } = JSON.parse(input);
const sockets = [];
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate) {
  for (let i=0; i<100; i++) { if (predicate()) return; await sleep(25); }
  throw new Error('Request notification timed out');
}
async function connect(token) {
  const socket = new WebSocket(base.replace(/^http/, 'ws')+'/ws/');
  sockets.push(socket);
  const events=[];
  socket.addEventListener('message', e => events.push(JSON.parse(e.data)));
  await new Promise((resolve,reject) => {
    socket.addEventListener('open',resolve,{once:true});
    socket.addEventListener('error',reject,{once:true});
  });
  socket.send(JSON.stringify({type:'authenticate',token}));
  await until(()=>events.some(e=>e.type==='authenticated'));
  return events;
}
try {
  const a=await connect(admin), b=await connect(admin), user=await connect(ordinary);
  const photo=await readFile(new URL('./fixtures/serial-proof.jpg',import.meta.url));
  for (const [kind,path] of [
    ['serial','/api/releases/1/serial-requests?serial=WS-98765'],
    ['alternative_name','/api/products/1/name-requests?name=Socket%20notification%20title'],
  ]) {
    const options={method:'POST',headers:{Authorization:`Bearer ${ordinary}`,'Content-Type':'image/jpeg'},body:photo};
    const response=await fetch(base+path,options);
    assert.equal(response.status,201);
    const {id}=await response.json();
    await until(()=>[a,b].every(events=>events.some(e=>e.type==='new_request'&&e.request_id===id&&e.kind===kind)));
    await sleep(100);
    const duplicate=await fetch(base+path,options);
    assert.equal(duplicate.status,409);
    await sleep(200);
    for (const events of [a,b]) assert.equal(events.filter(e=>e.type==='new_request'&&e.request_id===id).length,1);
  }
  assert.equal(user.filter(e=>e.type==='new_request').length,0);
  console.log('PASS: both request types notify every online admin tab, never ordinary users or failed submissions');
} finally { sockets.forEach(socket=>socket.close()); }
