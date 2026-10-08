#!/usr/bin/env python3
"""Compare stored counters with the actual catalogue COUNT query on an isolated DB.
Usage: python3 tests/platform_counts_contract.py CONTAINER DATABASE
The caller must apply migrations first. All fixture changes are rolled back.
"""
import json, re, subprocess, sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
source=(root/'src/product_list.rs').read_text()
def raw_function(name):
    body=source.split('fn '+name+'(',1)[1].split('\nfn ',1)[0]
    raw=re.search(r'r#"(.*?)"#',body,re.S)
    if raw: return raw.group(1)
    return re.search(r'format!\("(.*?)"',body,re.S).group(1)
visibility=raw_function('visibility_filter').replace('{unreleased}','false').replace('{platform}','checked_platform_id')
region=raw_function('build_region_filter').replace('{platform}','checked_platform_id').replace('{regions}','selected_regions').replace('{unreleased}','false')
# Extract the real handler's total_count SQL, not an independently rewritten approximation.
query=source.split('let count_sql = format!(',1)[1].split('r#"',1)[1].split('"#',1)[0]
for key,value in dict(region_filter=region,unknown_filter='',regional_columns='0::bigint AS unused',visibility=visibility,
 search_predicate='true',count_filter='').items(): query=query.replace('{'+key+'}',value)
# Empty search; no franchise/company/genre or multiplayer filters; released physical catalogue.
values={1:'checked_platform_id',2:"'%%'",3:'true',4:'NULL::integer',5:'NULL::integer',6:"'developer'",7:'false',8:'false',9:'false',10:'selected_regions',11:'NULL::integer'}
query=re.sub(r'\$(\d+)',lambda m:values[int(m[1])],query)
assert '{' not in query, query
sql="""
BEGIN;
CREATE TEMP TABLE results(platform integer,region text,expected bigint,stored bigint);
DO $test$
#variable_conflict use_column
DECLARE checked_platform_id integer; selected_regions text[]; region text; expected bigint; stored bigint;
BEGIN
 FOR checked_platform_id IN SELECT id FROM platforms ORDER BY id LOOP
  FOREACH region IN ARRAY ARRAY['total','europe','america','japan','other'] LOOP
   selected_regions:=CASE WHEN region='total' THEN ARRAY[]::text[] ELSE ARRAY[region] END;
   SELECT x.total INTO expected FROM (QUERY) x;
   EXECUTE format('SELECT %I FROM platforms WHERE id=$1',CASE WHEN region='total' THEN 'total_games' ELSE region||'_games' END) INTO stored USING checked_platform_id;
   INSERT INTO results VALUES(checked_platform_id,region,expected,stored);
  END LOOP;
 END LOOP;
 IF EXISTS(SELECT 1 FROM results WHERE expected IS DISTINCT FROM stored) THEN
   RAISE EXCEPTION 'Counter mismatch: %',(SELECT jsonb_agg(r) FROM results r WHERE expected IS DISTINCT FROM stored);
 END IF;
END $test$;
SELECT jsonb_build_object('checked',count(*),'platforms',count(DISTINCT platform),'mismatches',count(*) FILTER(WHERE expected IS DISTINCT FROM stored)) FROM results;
SELECT jsonb_agg(r) FROM results r WHERE platform IN (7,8,9,32,38,48,167);
ROLLBACK;
""".replace('QUERY',query)
p=subprocess.run(['docker','exec','-i',sys.argv[1],'psql','-X','-At','-U','postgres','-d',sys.argv[2],'-v','ON_ERROR_STOP=1'],input=sql,text=True,capture_output=True)
print(p.stdout); print(p.stderr,file=sys.stderr);sys.exit(p.returncode)
