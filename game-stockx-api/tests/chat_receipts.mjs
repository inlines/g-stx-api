import assert from 'node:assert/strict';
let input='';for await(const chunk of process.stdin)input+=chunk;
const {base,alice,bob,carol}=JSON.parse(input);
const sockets=[];const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(predicate){for(let i=0;i<150;i++){const value=predicate();if(value)return value;await sleep(25);}throw new Error('Chat receipt event timed out');}
async function connect(token){
 const socket=new WebSocket(base.replace(/^http/,'ws')+'/ws/');sockets.push(socket);const events=[];
 socket.addEventListener('message',e=>events.push(JSON.parse(e.data)));
 await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
 socket.send(JSON.stringify({type:'authenticate',token}));
 const auth=await until(()=>events.find(e=>e.type==='authenticated'));
 return {socket,events,login:auth.login,send:frame=>socket.send(JSON.stringify(frame))};
}
async function get(path,token){await sleep(75);const r=await fetch(base+path,{headers:{Authorization:`Bearer ${token}`}});assert.equal(r.status,200);return r.json();}
try{
 const a=await connect(alice),a2=await connect(alice),c=await connect(carol);
 const bobLogin=(await get('/api/profile/me',bob)).user_login;
 const before=await get('/api/chat/unread',bob);
 const client_id='receipt-test-one';
 a.send({sender:c.login,recipient:bobLogin,body:'Saved before delivery',client_id});
 const sent=await until(()=>a.events.find(e=>e.client_id===client_id&&e.id));
 assert.equal(sent.sender,a.login);assert.equal(sent.read,false);assert.equal(sent.read_at,null);
 await until(()=>a2.events.find(e=>e.id===sent.id));
 let history=await get('/api/messages?companion='+encodeURIComponent(a.login),bob);
 assert.equal(history.find(m=>m.id===sent.id).body,'Saved before delivery');
 const offline=await get('/api/chat/unread',bob);
 assert.equal(offline.unread[a.login],(before.unread[a.login]??0)+1);assert.ok(offline.revision>before.revision);
 a.send({recipient:bobLogin,body:'Saved before delivery',client_id});
 await until(()=>a.events.filter(e=>e.id===sent.id).length===2);
 assert.deepEqual(await get('/api/chat/unread',bob),offline);
 a.send({type:'read',reader:bobLogin,ids:[sent.id]});await sleep(120);
 assert.equal((await get('/api/messages?companion='+encodeURIComponent(a.login),bob)).find(m=>m.id===sent.id).read,false);
 const b=await connect(bob),b2=await connect(bob);
 assert.deepEqual(await get('/api/chat/unread',bob),offline);
 a.send({type:'typing',sender:c.login,recipient:b.login,typing:true});
 await until(()=>b.events.find(e=>e.type==='typing'&&e.sender===a.login&&e.typing));
 await until(()=>b2.events.find(e=>e.type==='typing'&&e.sender===a.login&&e.typing));
 assert.equal(c.events.filter(e=>e.type==='typing').length,0);
 a.send({type:'typing',recipient:b.login,typing:false});
 await until(()=>b.events.find(e=>e.type==='typing'&&!e.typing));
 b.send({type:'message',recipient:a.login,body:'Foreign recipient read guard',client_id:'receipt-test-two'});
 const foreign=await until(()=>b.events.find(e=>e.client_id==='receipt-test-two'&&e.id));
 b.send({type:'read',ids:[sent.id,foreign.id]});
 const receipt=await until(()=>a.events.find(e=>e.type==='read'&&e.ids.includes(sent.id)));
 assert.equal(receipt.reader,b.login);assert.ok(Number.isFinite(Date.parse(receipt.read_at)));assert.deepEqual(receipt.ids,[sent.id]);
 await until(()=>a2.events.find(e=>e.type==='read'&&e.ids.includes(sent.id)));
 await until(()=>b2.events.find(e=>e.type==='unread'&&e.revision>offline.revision));
 const after=await get('/api/chat/unread',bob);assert.equal(after.unread[a.login]??0,before.unread[a.login]??0);
 history=await get('/api/messages?companion='+encodeURIComponent(a.login),bob);
 assert.equal(history.find(m=>m.id===sent.id).read,true);assert.ok(history.find(m=>m.id===sent.id).read_at);
 assert.equal(history.find(m=>m.id===foreign.id).read,false);
 b.send({type:'read',ids:[sent.id]});await sleep(120);assert.deepEqual(await get('/api/chat/unread',bob),after);
 a.send({recipient:'no-such-chat-user',body:'Must fail',client_id:'receipt-test-failed'});
 await until(()=>a.events.find(e=>e.type==='send_failed'&&e.client_id==='receipt-test-failed'));
 b.socket.close();b2.socket.close();await sleep(150);
 const reconnected=await connect(bob);assert.equal(reconnected.login,b.login);assert.deepEqual(await get('/api/chat/unread',bob),after);
 console.log('PASS: persisted IDs, offline unread, recipient-bound reads, two-tab receipts/counts, idempotent sends/reads, reconnect, typing privacy and persistence failure');
}finally{sockets.forEach(s=>s.close());}
