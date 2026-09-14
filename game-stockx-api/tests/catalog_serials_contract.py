"""Run real generated serial-selection SQL on isolated TEMP fixtures; always roll back."""
from pathlib import Path
import subprocess, tempfile, sys, json
root=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
 source=Path(tmp)/'query.rs'
 source.write_text(f'#[path="{root}/src/catalog_serials.rs"] mod serials;\nfn main() {{ println!("SELECT p.id, s.groups, array_to_json({{}}) FROM products p CROSS JOIN selections s ORDER BY p.id,s.groups;",serials::selected_sql("9","s.groups")); }}')
 binary=Path(tmp)/'query'
 subprocess.run(['rustc',str(source),'-o',str(binary)],check=True)
 query=subprocess.check_output([str(binary)],text=True)
 setup="""BEGIN;
 CREATE TEMP TABLE products(id int); CREATE TEMP TABLE selections(groups text[]);
 CREATE TEMP TABLE releases(product_id int,platform int,release_region int,serial text[]);
 INSERT INTO products VALUES(1),(2),(3);
 INSERT INTO selections VALUES('{}'),('{europe}'),('{america}'),('{japan}'),('{europe,america}');
 INSERT INTO releases VALUES(1,9,8,ARRAY['BLUS-30026','BLUS-30026']),
 (1,8,1,ARRAY['SLES-12345']),
 (2,9,8,ARRAY['BLUS-30001']),(2,9,1,ARRAY['BLES-00001']),
 (2,9,5,ARRAY['BLJM-00001']),(3,9,1,'{}'),(3,9,8,ARRAY['BLUS-30002']);
 """
 output=subprocess.check_output(['docker','exec','-i',sys.argv[1],'psql','-XAtq','-U','postgres','-d',sys.argv[2],'-v','ON_ERROR_STOP=1'],input=setup+query+'ROLLBACK;',text=True)
 got={(int(i),g):json.loads(v) for i,g,v in (line.split('|') for line in output.splitlines())}
 assert got[1,'{europe}']==['BLUS-30026']
 assert got[1,'{}']==['BLUS-30026']
 assert got[2,'{europe}']==['BLES-00001']
 assert got[2,'{america}']==['BLUS-30001']
 assert got[2,'{japan}']==['BLJM-00001']
 assert got[2,'{europe,america}']==['BLES-00001','BLUS-30001']
 assert got[2,'{}']==['BLES-00001','BLJM-00001','BLUS-30001']
 assert got[3,'{europe}']==[]
print('PASS: region priority, Worldwide fallback, all regions, platform isolation, deduplication, empty exact release. Rolled back.')
