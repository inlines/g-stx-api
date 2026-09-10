"""Name contribution checks inside admin_contract's disposable PostgreSQL/Redis environment."""
import concurrent.futures
import json
import urllib.parse
from pathlib import Path

def exercise(request, sql, admin, uploader, jpeg):
    def submit(name='Japanese title', product=1, token=uploader, image=jpeg, status=201):
        return request(f'/api/products/{product}/name-requests?' + urllib.parse.urlencode({'name': name}), token, 'POST', image, status)
    def accept(id, value, token=admin, status=204):
        return request(f'/api/admin/serial-requests/{id}/accept', token, 'POST', json.dumps({'serial':value}).encode(), status)
    # The 20 pending serial requests created above share a limit with name requests.
    submit(status=409)
    sql("DELETE FROM release_serial_requests WHERE status='pending';")
    submit(token=None, status=401)
    submit(token='bad', status=401)
    submit(product=999999, status=404)
    submit(name=' ', status=400)
    submit(name='A'*201, status=400)
    submit(name='bad\x00name', status=400)
    submit(name=' fixture ', status=409)
    submit(image=b'not jpeg', status=400)
    submit(image=b'x'*(786432+1), status=413)
    sql("INSERT INTO product_platforms(product_id,platform_id,digital_only) VALUES(1,48,false) ON CONFLICT DO NOTHING;")
    query='/api/products?' + urllib.parse.urlencode({'cat':48,'limit':15,'offset':0,'query':'レーシング'})
    assert request(query)['total_count'] == 0  # Warm the empty search cache.
    assert not request('/api/products/1')['product']['alternative_names']
    first=submit(name='Wrong title')['id']
    row=next(x for x in request('/api/admin/serial-requests')['items'] if x['id']==first)
    assert row['kind']=='alternative_name' and row['release_id'] is None and row['product_id']==1
    assert row['existing_serials']==[]
    request(f'/api/admin/serial-requests/{first}/photo', uploader, status=403)
    assert request(f'/api/admin/serial-requests/{first}/photo') == jpeg
    accept(first,'レーシング — Новое имя',token=uploader,status=403)
    # Accepted title preserves Unicode and case. Concurrent retries award exactly five points.
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        list(executor.map(lambda _: accept(first,'  レーシング — Новое имя  '), range(2)))
    assert request('/api/kudos?login=victim')['kudos']==85
    assert request('/api/products/1')['product']['alternative_names']==['レーシング — Новое имя']
    assert request(query)['total_count']==1
    assert int(sql("SELECT id FROM alternative_names WHERE product_id=1"))<0
    # Personal/public collection DTOs include aliases without multiplying releases.
    sql("INSERT INTO covers(id,image_url) VALUES(987654,'alias-fixture'); UPDATE products SET cover_id=987654 WHERE id=1;")
    for path in ['/api/collection?cat=48', '/api/wishlist?cat=48', '/api/collection-by-login/victim?cat=0']:
        response = request(path, uploader)
        items = response['items'] if isinstance(response, dict) else response
        assert len(items) == 1, (path, items)
        assert items[0]['alternative_names'] == ['レーシング — Новое имя'], (path, items)

    archived=next(x for x in request('/api/admin/serial-requests?status=accepted&limit=50')['items'] if x['id']==first)
    assert archived['submitted_serial']=='Wrong title' and archived['serial']=='レーシング — Новое имя'
    accept(first,'Something different',status=409)
    submit(name='レーシング — новое имя',status=409)
    duplicate=submit(name='second pending')['id']
    submit(name=' SECOND PENDING ',status=409)
    accept(duplicate,'レーシング — Новое имя',status=409)
    assert request('/api/kudos?login=victim')['kudos']==85
    request(f'/api/admin/serial-requests/{duplicate}', method='DELETE',status=204)
    request(f'/api/admin/serial-requests/{duplicate}/photo',status=404)
    request(f'/api/admin/serial-requests/{first}/archive', uploader, 'DELETE',status=403)
    request(f'/api/admin/serial-requests/{first}/archive',method='DELETE',status=204)
    request(f'/api/admin/serial-requests/{first}/photo',status=404)
    assert request('/api/kudos?login=victim')['kudos']==85
    assert request(query)['total_count']==1
    # Migration replay preserves awards, catalogue data and the local ID sequence.
    migration=Path(__file__).resolve().parents[1]/'migrations/2026-09-10-180000-0000_name_requests/up.sql'
    sql(migration.read_text());sql(migration.read_text())
    long_name='名'*200
    second=submit(name=long_name)['id']; accept(second,long_name)
    assert request('/api/kudos?login=victim')['kudos']==90
    assert sql('SELECT count(DISTINCT id) FROM alternative_names WHERE product_id=1')=='2'
    # Failure of the final write rolls back name/status/cache revision and reward together.
    pending=submit(name='Rollback example')['id']
    revision=sql('SELECT revision FROM catalog_name_revision WHERE id=1')
    sql("CREATE FUNCTION reject_name_award() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.points=5 THEN RAISE EXCEPTION 'test rollback'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_name_award BEFORE INSERT ON kudos_awards FOR EACH ROW EXECUTE FUNCTION reject_name_award();")
    accept(pending,'Rollback example',status=500)
    sql('DROP TRIGGER test_name_award ON kudos_awards; DROP FUNCTION reject_name_award();')
    assert sql("SELECT count(*) FROM alternative_names WHERE name='Rollback example'")=='0'
    assert sql(f'SELECT status FROM release_serial_requests WHERE id={pending}')=='pending'
    assert sql('SELECT revision FROM catalog_name_revision WHERE id=1')==revision
    assert request('/api/kudos?login=victim')['kudos']==90
    print('PASS: name requests Unicode/edit/photo/auth, common limit, duplicate rejection, cached details/search refresh, concurrent +5 once, rollback, archive deletion, migration replay and negative IDs')
