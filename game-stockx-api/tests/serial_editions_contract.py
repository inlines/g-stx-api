"""Exercise generated edition SQL against temporary PostgreSQL fixtures.
Usage: python3 tests/serial_editions_contract.py [database, default postgres]
"""
from pathlib import Path
import subprocess, tempfile, sys, json
root=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
 source=Path(tmp)/'query.rs';binary=Path(tmp)/'query'
 source.write_text(f'#[path="{root}/src/catalog_serials.rs"] mod serials; fn main() {{ println!("SELECT s.groups, {{}} FROM products p CROSS JOIN selections s ORDER BY s.groups;", serials::editions_sql("9","s.groups")); }}')
 subprocess.run(['rustc','-A','dead_code',str(source),'-o',str(binary)],check=True)
 query=subprocess.check_output([str(binary)],text=True).replace('format_release_serials(', 'pg_temp.format_release_serials(')
 setup="""BEGIN; SET LOCAL search_path=pg_temp;
 CREATE TEMP TABLE products(id int); CREATE TEMP TABLE selections(groups text[]);
 CREATE TEMP TABLE releases(id int,product_id int,platform int,release_region int,serial text[]);
 CREATE TEMP TABLE release_serial_editions(release_id int,serial text,edition text);
 CREATE TEMP VIEW catalog_visible_releases AS SELECT * FROM releases;
 CREATE FUNCTION pg_temp.format_release_serials(text[]) RETURNS text[] LANGUAGE sql IMMUTABLE AS 'SELECT $1';
 INSERT INTO products VALUES(1);
 INSERT INTO selections VALUES('{}'),('{europe}'),('{america}'),('{japan}');
 INSERT INTO releases VALUES(1,1,9,1,ARRAY['BLES-01093']),(2,1,9,2,ARRAY['BLES-01093']),
 (3,1,9,8,ARRAY['BLES-00002']),(4,1,8,1,ARRAY['BLES-01093']);
 INSERT INTO release_serial_editions VALUES(1,'BLES-01093','platinum'),(1,'BLES-01093','essentials'),
 (1,'REMOVED','platinum'),(3,'BLES-00002','platinum'),(4,'BLES-01093','standard');
 """
 out=subprocess.check_output(['psql','-XAtq','-d',sys.argv[1] if len(sys.argv)>1 else 'postgres','-v','ON_ERROR_STOP=1'],input=setup+query+'ROLLBACK;',text=True)
 got={g:json.loads(v) for g,v in (line.split('|') for line in out.splitlines())}
 assert len(got['{europe}'])==2
 assert {a['edition'] for a in got['{europe}']}=={'platinum','essentials'}
 assert got['{america}']==[], 'Same code in another region must not inherit the badge'
 assert got['{japan}']==[{'serial':'BLES-00002','edition':'platinum'}]
 assert len(got['{}'])==3
 print('PASS: shared codes, platform/region isolation, WW fallback, removed-code exclusion. Rolled back.')
