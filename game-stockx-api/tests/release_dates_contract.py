"""Exercise real Rust-generated SQL on PostgreSQL TEMP tables only (always ROLLBACK).
Usage: python3 tests/release_dates_contract.py CONTAINER DATABASE
"""
from pathlib import Path
import subprocess, tempfile, sys
root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
    source = Path(tmp) / 'query.rs'
    source.write_text(f'#[path="{root}/src/release_dates.rs"] mod dates;\n' + r'''
fn main() {
 println!("SELECT p.id, platform.id, chosen.groups, {} AS release_date FROM products p CROSS JOIN platforms platform CROSS JOIN selections chosen CROSS JOIN LATERAL (SELECT {} AS dates OFFSET 0) release_dates ORDER BY chosen.groups,platform.id,release_date NULLS LAST,p.id;", dates::selected_sql("release_dates.dates", "chosen.groups"),dates::map_sql("p", "platform.id"));
}
''')
    binary = Path(tmp) / 'query'
    subprocess.run(['rustc',str(source),'-o',str(binary)],check=True)
    query = subprocess.check_output([str(binary)],text=True)
    setup = """BEGIN;
CREATE TEMP TABLE products(id int,first_release_date int);
CREATE TEMP TABLE platforms(id int);
CREATE TEMP TABLE releases(product_id int,platform int,release_region int,release_date int);
CREATE TEMP TABLE selections(groups text[]);
INSERT INTO products VALUES(1,10),(2,20),(3,30),(4,NULL),(5,0);
INSERT INTO platforms VALUES(48),(167);
INSERT INTO selections VALUES('{}'),('{europe}'),('{america}'),('{japan}'),('{america,europe}');
INSERT INTO releases VALUES (1,48,1,300),(1,48,1,350),(1,48,2,400),(1,48,8,100),(1,48,5,NULL),(1,167,1,50),(2,48,1,200),(3,48,5,600),(5,48,8,700);
"""
    result = subprocess.run(['docker','exec','-i',sys.argv[1],'psql','-XAtq','-U','postgres','-d',sys.argv[2],'-v','ON_ERROR_STOP=1'],input=setup+query+'ROLLBACK;',text=True,capture_output=True,check=True)
    rows=[r.split('|') for r in result.stdout.splitlines()]
    got={(int(g),int(p),regions):int(date) if date else None for g,p,regions,date in rows}
    assert got[1,48,'{europe}']==300
    assert got[1,48,'{america}']==400
    assert got[1,48,'{america,europe}']==300
    assert got[1,48,'{japan}']==100
    assert got[1,48,'{}']==100
    assert got[1,167,'{europe}']==50
    assert got[1,167,'{japan}']==10
    assert got[3,48,'{europe}']==30
    assert got[4,48,'{}'] is None
    assert got[5,167,'{}']==0
    assert [int(g) for g,p,r,d in rows if p=='48' and r=='{europe}']==[3,2,1,5,4]
    assert [int(g) for g,p,r,d in rows if p=='48' and r=='{}']==[1,2,3,5,4]
print('PASS: platform isolation, exact region priority, multiple regions, worldwide/game fallback, nulls, epoch zero and contextual ordering. TEMP tables rolled back.')
