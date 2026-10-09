"""Regional physical filtering and badges against real PostgreSQL; rollback-only fixtures."""
import re,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
s=(root/'src/product_list.rs').read_text()
def raw(name):
 body=s.split('fn '+name+'(',1)[1].split('\nfn ',1)[0]
 return re.search(r'format!\(\s*"(.*?)"',body,re.S).group(1)
f=raw('physical_region_filter').replace('{platform}','9').replace('{regions}',"ARRAY['europe']::text[]")
sql="""BEGIN;
CREATE TEMP TABLE products(id int);
CREATE TEMP TABLE releases(product_id int,platform int,release_region int,digital_only bool,release_status int,release_date int,serial text[]);
INSERT INTO products VALUES(1),(2),(3),(4),(5);
INSERT INTO releases VALUES
(1,9,1,true,NULL,NULL,'{}'),(1,9,2,false,NULL,NULL,ARRAY['BLUS-10001']), (1,9,8,false,NULL,NULL,'{}'),
(2,9,1,false,NULL,NULL,'{}'),(2,9,2,false,NULL,NULL,ARRAY['BLUS-10002']),
(3,9,8,false,NULL,NULL,'{}'),
(4,9,1,false,5,NULL,'{}'),(4,9,8,false,NULL,NULL,'{}'),
(5,9,1,false,NULL,2100000000,'{}');
SELECT array_agg(p.id ORDER BY p.id) FROM products p WHERE true FILTER;
ROLLBACK;
""".replace('FILTER',f)
r=subprocess.check_output(['docker','exec','-i',sys.argv[1],'psql','-XAtq','-U','postgres','-d',sys.argv[2],'-v','ON_ERROR_STOP=1'],input=sql,text=True).strip()
assert r=='{2,3}',r
print('PASS: exact digital excludes physical WW/foreign fallback; undated physical allowed; cancelled/future excluded.')
