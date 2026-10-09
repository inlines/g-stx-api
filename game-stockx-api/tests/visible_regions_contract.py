"""Verify hidden release isolation without deleting source data, on rollback fixtures."""
import re, subprocess, sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
s=(root/'src/product_list.rs').read_text()
def raw(name):
 body=s.split('fn '+name+'(',1)[1].split('\nfn ',1)[0]
 return re.search(r'format!\(\s*"(.*?)"',body,re.S).group(1)
physical=raw('physical_region_filter')
codes=s.split('let codes = format!(',1)[1].split('"',1)[1].split('"',1)[0]
setup="""BEGIN;
CREATE TEMP TABLE products(id int);
CREATE TEMP TABLE releases(id int,product_id int,platform int,release_region int,digital_only bool,release_status int,release_date bigint,serial text[]);
CREATE TEMP VIEW catalog_visible_releases AS SELECT * FROM releases WHERE release_region IN (1,2,5,8);
INSERT INTO products VALUES(1),(2),(3),(4);
INSERT INTO releases VALUES
(1,1,9,3,false,NULL,NULL,ARRAY['AU-CODE']),
(2,2,9,1,false,NULL,NULL,'{}'),(3,2,9,3,false,NULL,NULL,ARRAY['AU-CODE']),
(4,3,9,8,false,NULL,NULL,'{}'),(5,3,9,10,false,NULL,NULL,ARRAY['BR-CODE']),
(6,4,9,8,false,NULL,NULL,ARRAY['WW-CODE']), (7,4,9,7,false,NULL,NULL,'{}');
"""
queries=[]
for region in ['europe','america','japan','other']:
 present=physical.replace('{platform}','9').replace('{regions}',"ARRAY[unknown_region.region]::text[]")
 known=codes.replace('{platform}','9')
 queries.append(f"SELECT '{region}',array_agg(p.id ORDER BY p.id),array_agg(p.id ORDER BY p.id) FILTER(WHERE NOT {known}) FROM products p CROSS JOIN (VALUES('{region}')) unknown_region(region) WHERE true {present};")
queries.append('SELECT count(*) FROM releases;')
r=subprocess.run(['docker','exec','-i',sys.argv[1],'psql','-XAtq','-U','postgres','-d',sys.argv[2],'-v','ON_ERROR_STOP=1'],input=setup+'\n'.join(queries)+'ROLLBACK;',text=True,capture_output=True,check=True)
assert r.stdout.strip().splitlines()==['europe|{2,3,4}|{2,3}','america|{3,4}|{3}','japan|{3,4}|{3}','other|{3,4}|{3}','7'],r.stdout
print('PASS: hidden-only games excluded; hidden codes neither satisfy Unknown nor add Other; Worldwide fallback preserved; all seven source rows retained.')
