import assert from 'node:assert/strict';
let input=''; for await (const chunk of process.stdin) input+=chunk;
const {base,admin,victim}=JSON.parse(input), sockets=[];
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function user(login) {
 const r=await fetch(`${base}/api/admin/users?query=${login}`,{headers:{Authorization:`Bearer ${admin}`}});
 assert.equal(r.status,200);return (await r.json()).items.find(u=>u.user_login===login);
}
async function until(check,attempts=100,delay=50){for(let i=0;i<attempts;i++){if(await check())return;await pause(delay);}throw Error('last seen timeout');}
async function connect(token){
 const ws=new WebSocket(base.replace(/^http/,'ws')+'/ws/');sockets.push(ws);const events=[];
 ws.addEventListener('message',e=>events.push(JSON.parse(e.data)));
 await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',reject,{once:true});});
 ws.send(JSON.stringify({type:'authenticate',token}));return {ws,events};
}
try {
 assert.equal((await user('victim')).last_seen_at,null);
 const beforeAdmin=(await user('segasanshiro')).last_seen_at;
 const invalid=await connect('invalid');await until(()=>invalid.ws.readyState===WebSocket.CLOSED);
 assert.equal((await user('victim')).last_seen_at,null);
 const a=await connect(victim),b=await connect(victim);
 await until(()=>a.events.some(x=>x.type==='authenticated')&&b.events.some(x=>x.type==='authenticated'));
 await until(async()=>!!(await user('victim')).last_seen_at);
 const first=(await user('victim')).last_seen_at;
 assert.ok(Date.now()-Date.parse(first)<10000);
 a.ws.send(JSON.stringify({type:'typing',sender:'segasanshiro',recipient:'ordinary',typing:true}));
 await pause(100);assert.equal((await user('segasanshiro')).last_seen_at,beforeAdmin,'identity must come from authenticated socket');
 a.ws.close();await until(()=>a.ws.readyState===WebSocket.CLOSED);
 assert.ok(b.events.filter(x=>x.type==='presence').at(-1).online.includes('victim'),'remaining tab keeps user online');
 // Browsers answer protocol pings automatically even when the sidebar is closed.
 await until(async()=>Date.parse((await user('victim')).last_seen_at)>Date.parse(first),75,1000);
 const latest=(await user('victim')).last_seen_at;
 b.ws.close();await until(()=>b.ws.readyState===WebSocket.CLOSED);
 assert.equal((await user('victim')).last_seen_at,latest,'offline history remains available');
 console.log('PASS: private nullable history, authenticated identity, multiple tabs, heartbeat refresh and disconnect persistence');
} finally {sockets.forEach(s=>s.close());}
