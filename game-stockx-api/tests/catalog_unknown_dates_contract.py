"""Real HTTP tests on disposable PostgreSQL/Redis; never uses the project .env or an existing database."""
from pathlib import Path
import subprocess, tempfile, os, socket, time, json, urllib.request, urllib.error, sys
ROOT=Path(__file__).resolve().parents[1]
def run(args,**kw):
 result=subprocess.run(args,text=True,capture_output=True,**kw)
 if result.returncode: raise RuntimeError(str(args)+result.stdout+result.stderr)
 return result.stdout.strip()
pg='gstx-undated-catalog-contract-pg'; redis='gstx-undated-catalog-contract-redis'; containers=[]; process=None
try:
 for name,image,port,env in [(pg,'postgres:15','5432',['-e','POSTGRES_HOST_AUTH_METHOD=trust']),(redis,'redis:alpine','6379',[])]:
  run(['docker','run','-d','--name',name,'-p',f'127.0.0.1::{port}',*env,image]);containers.append(name)
 def port(name,p):return run(['docker','port',name,p]).rsplit(':',1)[1]
 for _ in range(100):
  if subprocess.run(['docker','exec',pg,'pg_isready','-h','127.0.0.1','-U','postgres'],capture_output=True).returncode==0:break
  time.sleep(.1)
 def sql(s):return run(['docker','exec','-i',pg,'psql','-XAtq','-U','postgres','-v','ON_ERROR_STOP=1'],input=s)
 probe=socket.socket();probe.bind(('127.0.0.1',0));http=probe.getsockname()[1];probe.close()
 env={**os.environ,'DATABASE_URL':f'postgres://postgres@127.0.0.1:{port(pg,"5432")}/postgres','REDIS_URL':f'redis://127.0.0.1:{port(redis,"6379")}','BIND_ADDRESS':f'127.0.0.1:{http}','RUST_LOG':'error'}
 with tempfile.TemporaryDirectory() as tmp:
  run(['diesel','migration','run','--config-file','/dev/null','--migration-dir',str(ROOT/'migrations')],env=env,cwd=tmp)
  log=open('/tmp/library-contract-server.log','w')
  process=subprocess.Popen([str(ROOT/'target/debug/game-stockx-api')],env=env,cwd=tmp,stdout=log,stderr=log)
  for _ in range(100):
   try:urllib.request.urlopen(f'http://127.0.0.1:{http}/health',timeout=.2).close();break
   except Exception:time.sleep(.1)
  token=None
  def req(path,body=None,auth=True,status=200):
   time.sleep(.12)
   headers={'Content-Type':'application/json'}
   if auth and token:headers['Authorization']='Bearer '+token
   request=urllib.request.Request(f'http://127.0.0.1:{http}/api'+path,data=None if body is None else json.dumps(body).encode(),headers=headers)
   try:r=urllib.request.urlopen(request,timeout=15)
   except urllib.error.HTTPError as e:r=e
   text=r.read().decode();assert r.status==status,(path,r.status,text)
   try:return json.loads(text)
   except ValueError:return text
  for name in ['alice','bob']:req('/register',{'user_login':name,'password':'testpassword'},False,status=201)
  token=req('/login',{'user_login':'alice','password':'testpassword'},False)['token']
  sql("INSERT INTO platforms(id,name,abbreviation,active) VALUES(48,'PlayStation 4','PS4',true); INSERT INTO regions(id,name) VALUES(1,'Europe'),(5,'Japan');")
  sql("UPDATE users SET is_admin=true WHERE user_login='alice'")
  from catalog_visibility_contract import exercise
  exercise(lambda path:req(path.removeprefix('/api')),sql)
finally:
 if process:process.terminate();process.wait(timeout=10)
 for name in containers:subprocess.run(['docker','rm','-f',name],capture_output=True)
