"""Real HTTP tests on disposable PostgreSQL/Redis; never uses the project .env or an existing database."""
from pathlib import Path
import subprocess, tempfile, os, socket, time, json, urllib.request, urllib.error, sys
ROOT=Path(__file__).resolve().parents[1]
def run(args,**kw):
 result=subprocess.run(args,text=True,capture_output=True,**kw)
 if result.returncode: raise RuntimeError(str(args)+result.stdout+result.stderr)
 return result.stdout.strip()
pg='gstx-library-contract-pg'; redis='gstx-library-contract-redis'; containers=[]; process=None
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
  sql("""INSERT INTO platforms(id,name,abbreviation,active) VALUES(48,'PlayStation 4','PS4',true),(167,'PlayStation 5','PS5',true);
  INSERT INTO regions(id,name) VALUES(1,'europe'),(2,'north_america'),(8,'worldwide');
  INSERT INTO products(id,summary,name,total_rating) SELECT i,'','Game '||lpad(i::text,3,'0'),i FROM generate_series(1,55) i;
  INSERT INTO releases(id,product_id,platform,release_region,serial,release_date) SELECT i,i,CASE WHEN i=55 THEN 167 ELSE 48 END,CASE WHEN i=54 THEN 8 WHEN i>50 THEN 2 ELSE 1 END,ARRAY['CUSA-'||lpad(i::text,5,'0'),'CUSA-'||lpad((i+100)::text,5,'0')],1000+i FROM generate_series(1,55) i;
  INSERT INTO product_platforms(product_id,platform_id) SELECT product_id,platform FROM releases;
  INSERT INTO users_have_releases(release_id,user_login,price) SELECT i,'alice',i*100 FROM generate_series(1,55) i;
  INSERT INTO users_have_releases(release_id,user_login,price) VALUES(1,'bob',999);
  INSERT INTO users_have_wishes(release_id,user_login) SELECT i,'alice' FROM generate_series(1,55) i;
  INSERT INTO users_have_wts(release_id,user_login,price,cib) SELECT i,'alice',i*200,i=1 FROM generate_series(1,55) i;
  """)
  migration=ROOT/'migrations/2026-09-23-140000-0000_collection_copy_details'
  sql((migration/'down.sql').read_text()+(migration/'up.sql').read_text())
  assert sql("SELECT cib FROM users_have_releases WHERE user_login='alice' AND release_id=1")=='t'
  assert sql("SELECT cib IS NULL FROM users_have_releases WHERE user_login='alice' AND release_id=2")=='t'
  for kind in ['collection','wishlist','wts']:
   first=req(f'/library/{kind}?cat=48&limit=24&offset=0');second=req(f'/library/{kind}?cat=48&limit=24&offset=24')
   assert first['total_count']==54 and second['total_count']==54
   assert [i['release_id'] for i in first['items']]==list(range(1,25))
   assert [i['release_id'] for i in second['items']]==list(range(25,49))
   assert first['items'][0]['serial']==['CUSA-00001','CUSA-00101']
   assert first['platform_ids']==[48,167]
   assert req(f'/library/{kind}?cat=48&query=Game%20053')['total_count']==1
   assert req(f'/library/{kind}?cat=48&search_mode=serial&query=CUSA00153')['items'][0]['release_id']==53
   assert req(f'/library/{kind}?cat=48&regions=europe')['total_count']==51
   assert req(f'/library/{kind}?cat=48&offset=999')['items']==[]
  sql("UPDATE product_platforms SET digital_only=true WHERE product_id=1 AND platform_id=48")
  digital=req('/library/collection?cat=48&limit=1')
  assert digital['items'][0]['digital_only'] is True
  assert digital['owned_regions']['europe']==50
  sql("UPDATE product_platforms SET digital_only=false WHERE product_id=1 AND platform_id=48")
  req('/collection-copy',{'release_id':1,'selected_serial':'cusa00101','cib':False})
  for kind in ['collection','wts']:
   row=req(f'/library/{kind}?query=Game%20001')['items'][0]
   assert row['selected_serial']=='CUSA-00101' and row['cib'] is False and row['purchase_price']==100
   assert row['price']==(100 if kind=='collection' else 200)
  req('/collection-copy',{'release_id':1,'selected_serial':'CUSA-00002','cib':True},status=400)
  assert req('/library/collection?query=Game%20001')['items'][0]['cib'] is False
  req('/collection-copy',{'release_id':999,'selected_serial':None,'cib':True},status=404)
  req('/collection-copy',{'release_id':1,'selected_serial':None,'cib':None},auth=False,status=401)
  req('/collection-copy',{'release_id':1,'selected_serial':None,'cib':None})
  assert req('/library/collection?query=Game%20001')['items'][0]['cib'] is None
  req('/add_wts',{'release_id':1,'price':777,'cib':True})
  assert req('/library/collection?query=Game%20001')['items'][0]['cib'] is True
  public=req('/library/collection?login=bob')['items'][0]
  assert public['purchase_price'] is None and public['price'] is None and public['selected_serial'] is None
  req('/library/wishlist?login=bob',status=400)
  req('/library/collection?limit=0',status=400)
  req('/library/collection?sort=invalid',status=400)
  token=req('/login',{'user_login':'bob','password':'testpassword'},False)['token']
  req('/collection-copy',{'release_id':2,'selected_serial':'CUSA-00002','cib':True},status=404)
  print('PASS: migration/backfill, pagination/order/counts, global search/filters, exact serial membership, CIB tri-state, isolated ownership, purchase/sale prices, public privacy and legacy WTS compatibility')
finally:
 if process:process.terminate();process.wait(timeout=10)
 for name in containers:subprocess.run(['docker','rm','-f',name],capture_output=True)
