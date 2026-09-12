"""Worldwide cleanup runs transactionally and never cascades user relationships."""
from pathlib import Path

def exercise(sql):
    migration = Path(__file__).resolve().parents[1] / 'migrations/2026-09-12-140000-0000_worldwide_serial_cleanup'
    up, down = [(migration / name).read_text() for name in ['up.sql', 'down.sql']]
    sql('BEGIN;'+down+'COMMIT;')
    sql("""INSERT INTO products(id,name,summary) VALUES(750,'Worldwide test',''),(751,'Other game','');
      INSERT INTO releases(id,product_id,platform,release_region,serial) VALUES
      (750,750,48,1,ARRAY['CUSA-75001']), (751,750,48,5,ARRAY['CUSA-75002']),
      (752,750,167,1,ARRAY['PPSA-75003']),
      (753,750,48,8,ARRAY['cusa75001','CUSA-75002','PPSA-75003','CUSA-75004']),
      (754,750,48,8,ARRAY['CUSA-75001','CUSA-75002']),
      (755,750,48,8,ARRAY['CUSA-75001']), (756,750,48,8,ARRAY['CUSA-75001']),
      (757,750,48,8,ARRAY['CUSA-75001']), (758,750,48,8,ARRAY['CUSA-75001']),
      (759,751,48,8,ARRAY['CUSA-75001']), (760,750,48,8,ARRAY[]::text[]);
      INSERT INTO users_have_releases(release_id,user_login) VALUES(755,'ordinary');
      INSERT INTO users_have_wishes(release_id,user_login) VALUES(756,'ordinary');
      INSERT INTO users_have_wts(release_id,user_login) VALUES(757,'ordinary');
      INSERT INTO release_serial_requests(release_id,submitter_id,serial,photo)
      SELECT 758,id,'CUSA-75999',decode('00','hex') FROM users WHERE user_login='ordinary';
    """)
    fingerprint = "SELECT md5(string_agg(row(r.*)::text,'|' ORDER BY id)) FROM releases r WHERE id BETWEEN 750 AND 760"
    original=sql(fingerprint)
    sql('BEGIN;'+up+'COMMIT;')
    assert sql('SELECT serial::text FROM releases WHERE id=753')=='{PPSA-75003,CUSA-75004}'
    assert sql('SELECT count(*) FROM releases WHERE id=754')=='0'
    assert sql('SELECT count(*) FROM releases WHERE id BETWEEN 755 AND 758 AND cardinality(serial)=0')=='4'
    assert sql('SELECT serial::text FROM releases WHERE id=759')=='{CUSA-75001}'
    assert sql('SELECT count(*) FROM releases WHERE id=760')=='1', 'Do not remove unrelated pre-existing empty releases'
    assert sql('SELECT count(*) FROM release_serial_requests WHERE release_id=758')=='1'
    for table, release_id in [('users_have_releases',755),('users_have_wishes',756),('users_have_wts',757)]:
        assert sql(f'SELECT count(*) FROM {table} WHERE release_id={release_id}')=='1'
    cleaned=sql(fingerprint)
    # A new contribution must survive rollback. Use an outer rollback to leave this test independent.
    sql("BEGIN; UPDATE releases SET serial=array_append(serial,'CUSA-75998') WHERE id=753;"+down+"DO $$ BEGIN IF NOT EXISTS(SELECT 1 FROM releases WHERE id=753 AND 'CUSA-75998'=ANY(serial)) THEN RAISE EXCEPTION 'New serial lost'; END IF; END $$; ROLLBACK;")
    assert sql(fingerprint)==cleaned
    sql('BEGIN;'+down+'COMMIT;')
    assert sql(fingerprint)==original, 'Rollback must restore complete original rows and arrays'
    sql('BEGIN;'+up+'COMMIT;')
    assert sql(fingerprint)==cleaned
    sql('DELETE FROM products WHERE id IN(750,751);')
    print('PASS: Worldwide exact deduplication, same-game/platform isolation, unique serials, four relationship guards, deletion, rollback and reapply')
